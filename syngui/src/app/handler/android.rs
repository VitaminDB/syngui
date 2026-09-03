use super::AppHandler;

impl AppHandler {
    #[cfg(target_os = "android")]
    pub(super) fn update_safe_area(&mut self) {
        use crate::core::EdgeInsets;

        let Some(ref android_app) = self.android_app else { return };

        let vm_ptr = android_app.vm_as_ptr();
        let activity_ptr = android_app.activity_as_ptr();

        let result = unsafe { self.query_safe_area_jni(vm_ptr, activity_ptr) };
        match result {
            Ok(insets) => {
                self.tree.safe_area = insets;
            }
            Err(e) => {
                log::warn!("Failed to query safe area via JNI: {}, using defaults", e);
                self.tree.safe_area = EdgeInsets::new(0.0, 24.0, 0.0, 48.0);
            }
        }
    }

    #[cfg(target_os = "android")]
    unsafe fn query_safe_area_jni(
        &self,
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
    ) -> Result<crate::core::EdgeInsets, String> {
        use jni::objects::{JObject, JValue};

        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
            .map_err(|e| format!("JavaVM::from_raw: {}", e))?;
        let mut env = vm.attach_current_thread_permanently()
            .map_err(|e| format!("attach_thread: {}", e))?;

        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let resources = env.call_method(&activity, "getResources", "()Landroid/content/res/Resources;", &[])
            .map_err(|e| format!("getResources: {}", e))?
            .l().map_err(|e| format!("getResources cast: {}", e))?;

        let dimen_str: JObject = env.new_string("dimen").map_err(|e| format!("new_string: {}", e))?.into();
        let android_str: JObject = env.new_string("android").map_err(|e| format!("new_string: {}", e))?.into();

        let status_name: JObject = env.new_string("status_bar_height").map_err(|e| format!("new_string: {}", e))?.into();
        let status_id = env.call_method(
            &resources, "getIdentifier",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&status_name), JValue::Object(&dimen_str), JValue::Object(&android_str)],
        ).map_err(|e| format!("getIdentifier(status): {}", e))?
            .i().map_err(|e| format!("getIdentifier cast: {}", e))?;

        let status_px = if status_id > 0 {
            env.call_method(&resources, "getDimensionPixelSize", "(I)I", &[JValue::Int(status_id)])
                .map_err(|e| format!("getDimensionPixelSize(status): {}", e))?
                .i().map_err(|e| format!("getDimensionPixelSize cast: {}", e))?
        } else { 0 };

        let nav_name: JObject = env.new_string("navigation_bar_height").map_err(|e| format!("new_string: {}", e))?.into();
        let nav_id = env.call_method(
            &resources, "getIdentifier",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I",
            &[JValue::Object(&nav_name), JValue::Object(&dimen_str), JValue::Object(&android_str)],
        ).map_err(|e| format!("getIdentifier(nav): {}", e))?
            .i().map_err(|e| format!("getIdentifier cast: {}", e))?;

        let nav_px = if nav_id > 0 {
            env.call_method(&resources, "getDimensionPixelSize", "(I)I", &[JValue::Int(nav_id)])
                .map_err(|e| format!("getDimensionPixelSize(nav): {}", e))?
                .i().map_err(|e| format!("getDimensionPixelSize cast: {}", e))?
        } else { 0 };

        let sf = self.scale_factor as f32;
        Ok(crate::core::EdgeInsets::new(0.0, status_px as f32 / sf, 0.0, nav_px as f32 / sf))
    }

    #[cfg(target_os = "android")]
    pub(super) fn poll_android_back(&mut self) {
        use crate::input::Event;

        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();
        let activity_ptr = android_app.activity_as_ptr();

        let pressed = unsafe { Self::poll_back_jni(vm_ptr, activity_ptr) };
        if pressed {
            self.dispatch_back();
        }
    }

    /// Обработка жеста/клавиши «назад», общая для обоих каналов доставки:
    /// OnBackInvokedDispatcher (Android 13+, `poll_android_back`) и legacy
    /// KEYCODE_BACK → BrowserBack (KeyboardInput в event_handling). Сначала
    /// закрывается экранная клавиатура, затем событие идёт виджетам; если
    /// никто не обработал — приложение сворачивается.
    #[cfg(target_os = "android")]
    pub(crate) fn dispatch_back(&mut self) {
        use crate::input::Event;

        if self.keyboard_shown {
            self.keyboard_shown = false;
            self.composing_len = 0;
            self.hide_keyboard_jni();
            if let Some(old_id) = self.focus_manager.current_focus() {
                self.tree.dispatch_event_to(old_id, &Event::FocusLost);
                self.focus_manager.clear_focus();
                self.tree.focused_element = None;
            }
            self.process_virtual_keyboard_request();
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
            return;
        }

        let back_event = Event::BackPressed;
        let handled = self
            .root_id
            .map(|root_id| self.tree.handle_event(root_id, &back_event).is_handled())
            .unwrap_or(false);
        if handled {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        } else {
            // Некому обрабатывать «назад» — стандартное поведение Android:
            // приложение сворачивается (не завершается).
            self.move_task_to_back();
        }
    }

    /// `Activity.moveTaskToBack(true)`: свернуть приложение — реакция на жест
    /// «назад», который не обработал ни один виджет.
    #[cfg(target_os = "android")]
    pub(crate) fn move_task_to_back(&self) {
        use jni::objects::JObject;

        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();
        let activity_ptr = android_app.activity_as_ptr();
        if vm_ptr.is_null() || activity_ptr.is_null() {
            return;
        }

        unsafe {
            let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) else { return };
            let Ok(mut env) = vm.attach_current_thread_permanently() else {
                std::mem::forget(vm);
                return;
            };
            let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);
            if env
                .call_method(&activity, "moveTaskToBack", "(Z)Z", &[jni::objects::JValue::Bool(1)])
                .is_err()
            {
                let _ = env.exception_clear();
            }
            std::mem::forget(vm);
        }
    }

    #[cfg(target_os = "android")]
    const FRAMEWORK_DEX: &'static [u8] = include_bytes!("../android/syngui_framework.dex");

    #[cfg(target_os = "android")]
    fn back_class_cache() -> &'static std::sync::atomic::AtomicPtr<()> {
        static CACHE: std::sync::atomic::AtomicPtr<()> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        &CACHE
    }

    #[cfg(target_os = "android")]
    fn input_class_cache() -> &'static std::sync::atomic::AtomicPtr<()> {
        static CACHE: std::sync::atomic::AtomicPtr<()> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        &CACHE
    }

    #[cfg(target_os = "android")]
    fn notif_class_cache() -> &'static std::sync::atomic::AtomicPtr<()> {
        static CACHE: std::sync::atomic::AtomicPtr<()> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
        &CACHE
    }

    #[cfg(target_os = "android")]
    pub(super) fn register_back_handler(&self) {
        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();
        let activity_ptr = android_app.activity_as_ptr();

        if let Err(e) = unsafe { Self::load_and_register_back(vm_ptr, activity_ptr) } {
            log::warn!("Back handler registration failed: {}", e);
        }
    }

    #[cfg(target_os = "android")]
    unsafe fn load_and_register_back(
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
    ) -> Result<(), String> {
        use jni::objects::{JObject, JValue};

        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
            .map_err(|e| format!("JavaVM: {}", e))?;
        let mut env = vm.attach_current_thread_permanently()
            .map_err(|e| format!("attach: {}", e))?;

        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let dex = Self::FRAMEWORK_DEX;
        let buf = env.new_direct_byte_buffer(dex.as_ptr() as *mut u8, dex.len())
            .map_err(|e| format!("ByteBuffer: {}", e))?;

        let parent = env.call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .map_err(|e| format!("getClassLoader: {}", e))?
            .l().map_err(|e| format!("cast: {}", e))?;

        let loader_cls = env.find_class("dalvik/system/InMemoryDexClassLoader")
            .map_err(|e| format!("find InMemoryDexClassLoader: {}", e))?;
        let loader = env.new_object(
            loader_cls,
            "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
            &[JValue::Object(&buf), JValue::Object(&parent)],
        ).map_err(|e| format!("new InMemoryDexClassLoader: {}", e))?;

        let name: JObject = env.new_string("syngui.android.SynGuiBackHandler")
            .map_err(|e| format!("string: {}", e))?.into();
        let cls = env.call_method(
            &loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&name)],
        ).map_err(|e| format!("loadClass: {}", e))?
            .l().map_err(|e| format!("cast: {}", e))?;

        let cls_ref: jni::objects::JClass = cls.into();
        env.call_static_method(
            &cls_ref, "register", "(Landroid/app/Activity;)V",
            &[JValue::Object(&activity)],
        ).map_err(|e| format!("register: {}", e))?;

        let global = env.new_global_ref(cls_ref)
            .map_err(|e| format!("global_ref: {}", e))?;
        let raw = global.as_raw() as *mut ();
        std::mem::forget(global);
        Self::back_class_cache().store(raw, std::sync::atomic::Ordering::SeqCst);

        let input_name: JObject = env.new_string("syngui.android.SynGuiInputHandler")
            .map_err(|e| format!("string: {}", e))?.into();
        let input_cls = env.call_method(
            &loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&input_name)],
        ).map_err(|e| format!("loadClass(Input): {}", e))?
            .l().map_err(|e| format!("cast: {}", e))?;

        let input_cls_ref: jni::objects::JClass = input_cls.into();
        env.call_static_method(
            &input_cls_ref, "register", "(Landroid/app/Activity;)V",
            &[JValue::Object(&activity)],
        ).map_err(|e| format!("InputHandler.register: {}", e))?;

        let input_global = env.new_global_ref(input_cls_ref)
            .map_err(|e| format!("input global_ref: {}", e))?;
        let input_raw = input_global.as_raw() as *mut ();
        std::mem::forget(input_global);
        Self::input_class_cache().store(input_raw, std::sync::atomic::Ordering::SeqCst);

        let notif_name: JObject = env.new_string("syngui.android.SynGuiNotificationHandler")
            .map_err(|e| format!("string: {}", e))?.into();
        let notif_cls = env.call_method(
            &loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;",
            &[JValue::Object(&notif_name)],
        ).map_err(|e| format!("loadClass(Notif): {}", e))?
            .l().map_err(|e| format!("cast: {}", e))?;

        let notif_cls_ref: jni::objects::JClass = notif_cls.into();
        env.call_static_method(
            &notif_cls_ref, "register", "(Landroid/app/Activity;)V",
            &[JValue::Object(&activity)],
        ).map_err(|e| format!("NotifHandler.register: {}", e))?;

        let notif_global = env.new_global_ref(notif_cls_ref)
            .map_err(|e| format!("notif global_ref: {}", e))?;
        let notif_raw = notif_global.as_raw() as *mut ();
        std::mem::forget(notif_global);
        Self::notif_class_cache().store(notif_raw, std::sync::atomic::Ordering::SeqCst);

        super::super::notification::set_jni_ptrs(vm_ptr, notif_raw);

        std::mem::forget(vm);
        Ok(())
    }

    #[cfg(target_os = "android")]
    unsafe fn poll_back_jni(
        vm_ptr: *mut std::ffi::c_void,
        _activity_ptr: *mut std::ffi::c_void,
    ) -> bool {
        use jni::objects::JObject;

        let ptr = Self::back_class_cache().load(std::sync::atomic::Ordering::Relaxed);
        if ptr.is_null() { return false; }

        let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) else { return false; };
        let Ok(mut env) = vm.attach_current_thread_permanently() else {
            std::mem::forget(vm); return false;
        };

        let cls: jni::objects::JClass = JObject::from_raw(ptr as jni::sys::jobject).into();
        let result = env.call_static_method(cls, "consumeBack", "()Z", &[])
            .ok().and_then(|v| v.z().ok()).unwrap_or(false);

        std::mem::forget(vm);
        result
    }

    #[cfg(target_os = "android")]
    pub(super) fn poll_input_handler(&mut self) {
        use crate::input::Event;

        let Some(ref android_app) = self.android_app else { return };
        let ptr = Self::input_class_cache().load(std::sync::atomic::Ordering::Relaxed);
        if ptr.is_null() { return; }

        let vm_ptr = android_app.vm_as_ptr();
        let Ok(vm) = (unsafe { jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) }) else { return };
        let Ok(mut env) = vm.attach_current_thread_permanently() else {
            std::mem::forget(vm); return;
        };

        let cls: jni::objects::JClass = unsafe {
            jni::objects::JObject::from_raw(ptr as jni::sys::jobject).into()
        };

        let root_id = self.root_id;
        let mut had_events = false;

        loop {
            let result = env.call_static_method(&cls, "pollEvent", "()Ljava/lang/String;", &[]);
            let event_str = match result {
                Ok(val) => match val.l() {
                    Ok(obj) => {
                        if obj.is_null() { break; }
                        match env.get_string((&obj).into()) {
                            Ok(s) => s.to_string_lossy().to_string(),
                            Err(_) => break,
                        }
                    }
                    Err(_) => break,
                },
                Err(_) => break,
            };

            let Some(root_id) = root_id else { continue };
            had_events = true;

            if let Some(text) = event_str.strip_prefix("C:") {
                for _ in 0..self.composing_len {
                    self.tree.handle_event(root_id, &Event::KeyDown(crate::input::Key::Backspace));
                }
                self.composing_len = 0;
                for ch in text.chars() {
                    self.tree.handle_event(root_id, &Event::CharInput(ch));
                }
            } else if let Some(text) = event_str.strip_prefix("S:") {
                for _ in 0..self.composing_len {
                    self.tree.handle_event(root_id, &Event::KeyDown(crate::input::Key::Backspace));
                }
                self.composing_len = text.chars().count();
                for ch in text.chars() {
                    self.tree.handle_event(root_id, &Event::CharInput(ch));
                }
            } else if event_str.starts_with("F:") {
                self.composing_len = 0;
            } else if event_str.starts_with("R:") {
                for _ in 0..self.composing_len {
                    self.tree.handle_event(root_id, &Event::KeyDown(crate::input::Key::Backspace));
                }
                self.composing_len = 0;
            } else if let Some(params) = event_str.strip_prefix("D:") {
                let parts: Vec<&str> = params.split(',').collect();
                let before = parts.first().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                let after = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                for _ in 0..before {
                    self.tree.handle_event(root_id, &Event::KeyDown(crate::input::Key::Backspace));
                }
                for _ in 0..after {
                    self.tree.handle_event(root_id, &Event::KeyDown(crate::input::Key::Delete));
                }
            } else if event_str == "K:enter" {
                self.tree.handle_event(root_id, &Event::KeyDown(crate::input::Key::Enter));
            }
        }

        std::mem::forget(vm);

        if had_events {
            if let Some(ref window) = self.window {
                window.request_redraw();
            }
        }
    }

    #[cfg(target_os = "android")]
    fn show_keyboard_jni(&self) {
        let ptr = Self::input_class_cache().load(std::sync::atomic::Ordering::Relaxed);
        if ptr.is_null() { return; }
        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();

        unsafe {
            let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) else { return };
            let Ok(mut env) = vm.attach_current_thread_permanently() else {
                std::mem::forget(vm); return;
            };
            let cls: jni::objects::JClass = jni::objects::JObject::from_raw(ptr as jni::sys::jobject).into();
            let _ = env.call_static_method(&cls, "showKeyboard", "()V", &[]);
            std::mem::forget(vm);
        }
    }

    #[cfg(target_os = "android")]
    fn hide_keyboard_jni(&self) {
        let ptr = Self::input_class_cache().load(std::sync::atomic::Ordering::Relaxed);
        if ptr.is_null() { return; }
        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();

        unsafe {
            let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) else { return };
            let Ok(mut env) = vm.attach_current_thread_permanently() else {
                std::mem::forget(vm); return;
            };
            let cls: jni::objects::JClass = jni::objects::JObject::from_raw(ptr as jni::sys::jobject).into();
            let _ = env.call_static_method(&cls, "hideKeyboard", "()V", &[]);
            std::mem::forget(vm);
        }
    }

    /// Тип экранной клавиатуры: цифровая (true) или обычная (false).
    #[cfg(target_os = "android")]
    fn set_input_numeric_jni(&self, numeric: bool) {
        let ptr = Self::input_class_cache().load(std::sync::atomic::Ordering::Relaxed);
        if ptr.is_null() { return; }
        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();

        unsafe {
            let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) else { return };
            let Ok(mut env) = vm.attach_current_thread_permanently() else {
                std::mem::forget(vm); return;
            };
            let cls: jni::objects::JClass = jni::objects::JObject::from_raw(ptr as jni::sys::jobject).into();
            let _ = env.call_static_method(
                &cls,
                "setNumericInput",
                "(Z)V",
                &[jni::objects::JValue::Bool(numeric as u8)],
            );
            std::mem::forget(vm);
        }
    }

    #[cfg(target_os = "android")]
    fn set_input_text_jni(&self, text: &str) {
        let ptr = Self::input_class_cache().load(std::sync::atomic::Ordering::Relaxed);
        if ptr.is_null() { return; }
        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();

        unsafe {
            let Ok(vm) = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM) else { return };
            let Ok(mut env) = vm.attach_current_thread_permanently() else {
                std::mem::forget(vm); return;
            };
            let cls: jni::objects::JClass = jni::objects::JObject::from_raw(ptr as jni::sys::jobject).into();
            if let Ok(jtext) = env.new_string(text) {
                let jobj: jni::objects::JObject = jtext.into();
                let _ = env.call_static_method(&cls, "setText", "(Ljava/lang/String;)V",
                    &[jni::objects::JValue::Object(&jobj)]);
            }
            std::mem::forget(vm);
        }
    }

    /// Положение фокусного элемента в логических пикселях (CSS px на wasm).
    #[cfg(target_arch = "wasm32")]
    fn focused_element_rect(&self) -> Option<crate::core::Rect> {
        let id = self.focus_manager.current_focus().or(self.tree.focused_element)?;
        self.tree.get(id).map(|el| el.bounds())
    }

    /// Веб: положение агента ввода следует за фокусным полем (скролл,
    /// relayout под клавиатуру) — браузер прокручивает страницу именно к нему.
    #[cfg(target_arch = "wasm32")]
    pub(in crate::app) fn sync_web_text_agent_rect(&self) {
        if !crate::app::web_text_agent::is_shown() {
            return;
        }
        if let Some(rect) = self.focused_element_rect() {
            crate::app::web_text_agent::sync_rect(rect);
        }
    }

    /// Веб: пользователь закрыл экранную клавиатуру сам (кнопка «назад»).
    /// Как `dispatch_back` на Android — поле теряет фокус, чтобы следующий
    /// тап снова поднял клавиатуру.
    #[cfg(target_arch = "wasm32")]
    pub(in crate::app) fn dismiss_web_keyboard(&mut self) {
        use crate::input::Event;

        if let Some(old_id) = self.focus_manager.current_focus() {
            if self.tree.elements.contains_key(&old_id) {
                self.tree.dispatch_event_to(old_id, &Event::FocusLost);
            }
            self.focus_manager.clear_focus();
            self.tree.focused_element = None;
            self.a11y_dirty = true;
        }
        self.tree.virtual_keyboard_request = Some(false);
        self.process_virtual_keyboard_request();
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
    }

    pub(in crate::app) fn process_virtual_keyboard_request(&mut self) {
        if let Some(_show) = self.tree.virtual_keyboard_request.take() {
            #[cfg(target_arch = "wasm32")]
            {
                if _show {
                    let text = self.tree.focused_text_content.take();
                    let rect = self.focused_element_rect();
                    crate::app::web_text_agent::show(
                        text.as_deref(),
                        self.tree.keyboard_numeric,
                        self.tree.keyboard_secret,
                        rect,
                    );
                    // После сжатия viewport'а клавиатурой (Resized) поле
                    // прокручивается в видимую область.
                    self.pending_scroll_element = self.focus_manager.current_focus();
                } else {
                    crate::app::web_text_agent::hide();
                    self.pending_scroll_element = None;
                }
            }
            #[cfg(target_os = "android")]
            {
                if _show {
                    self.set_input_numeric_jni(self.tree.keyboard_numeric);
                    self.composing_len = 0;
                    if let Some(text) = self.tree.focused_text_content.take() {
                        self.set_input_text_jni(&text);
                    }
                    self.pending_scroll_element = self.focus_manager.current_focus();
                }
                if _show != self.keyboard_shown {
                    self.keyboard_shown = _show;
                    if _show {
                        self.show_keyboard_jni();
                    } else {
                        self.hide_keyboard_jni();
                        self.pending_scroll_element = None;
                    }
                }
            }
        }
    }

    #[cfg(target_os = "android")]
    unsafe fn toggle_soft_input_jni(
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
        show: bool,
    ) -> Result<(), String> {
        use jni::objects::{JObject, JValue};

        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
            .map_err(|e| format!("JavaVM::from_raw: {}", e))?;
        let mut env = vm.attach_current_thread_permanently()
            .map_err(|e| format!("attach_thread: {}", e))?;

        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let service_name: JObject = env.new_string("input_method")
            .map_err(|e| format!("new_string: {}", e))?.into();
        let imm = env.call_method(
            &activity, "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&service_name)],
        ).map_err(|e| format!("getSystemService: {}", e))?
            .l().map_err(|e| format!("getSystemService cast: {}", e))?;

        if show {
            env.call_method(&imm, "toggleSoftInput", "(II)V",
                &[JValue::Int(2), JValue::Int(0)],
            ).map_err(|e| format!("toggleSoftInput(show): {}", e))?;
        } else {
            let window = env.call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
                .map_err(|e| format!("getWindow: {}", e))?
                .l().map_err(|e| format!("getWindow cast: {}", e))?;
            let decor_view = env.call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
                .map_err(|e| format!("getDecorView: {}", e))?
                .l().map_err(|e| format!("getDecorView cast: {}", e))?;
            let token = env.call_method(&decor_view, "getWindowToken", "()Landroid/os/IBinder;", &[])
                .map_err(|e| format!("getWindowToken: {}", e))?
                .l().map_err(|e| format!("getWindowToken cast: {}", e))?;
            env.call_method(&imm, "hideSoftInputFromWindow", "(Landroid/os/IBinder;I)Z",
                &[JValue::Object(&token), JValue::Int(0)],
            ).map_err(|e| format!("hideSoftInputFromWindow: {}", e))?;
        }

        std::mem::forget(vm);
        Ok(())
    }

    #[cfg(target_os = "android")]
    pub(super) fn query_keyboard_height(&self) -> f32 {
        let Some(ref android_app) = self.android_app else { return 0.0 };
        let vm_ptr = android_app.vm_as_ptr();
        let activity_ptr = android_app.activity_as_ptr();
        let sf = self.scale_factor as f32;

        match unsafe { Self::get_keyboard_height_jni(vm_ptr, activity_ptr, sf) } {
            Ok(h) => h,
            Err(e) => {
                log::warn!("query_keyboard_height failed: {}", e);
                0.0
            }
        }
    }

    #[cfg(target_os = "android")]
    unsafe fn get_keyboard_height_jni(
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
        scale_factor: f32,
    ) -> Result<f32, String> {
        use jni::objects::{JObject, JValue};

        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
            .map_err(|e| format!("JavaVM::from_raw: {}", e))?;
        let mut env = vm.attach_current_thread_permanently()
            .map_err(|e| format!("attach_thread: {}", e))?;

        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let window = env.call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
            .map_err(|e| format!("getWindow: {}", e))?
            .l().map_err(|e| format!("getWindow cast: {}", e))?;

        let decor_view = env.call_method(&window, "getDecorView", "()Landroid/view/View;", &[])
            .map_err(|e| format!("getDecorView: {}", e))?
            .l().map_err(|e| format!("getDecorView cast: {}", e))?;

        let root_height = env.call_method(&decor_view, "getHeight", "()I", &[])
            .map_err(|e| format!("getHeight: {}", e))?
            .i().map_err(|e| format!("getHeight cast: {}", e))?;

        let rect_class = env.find_class("android/graphics/Rect")
            .map_err(|e| format!("find Rect class: {}", e))?;
        let rect = env.new_object(&rect_class, "()V", &[])
            .map_err(|e| format!("new Rect: {}", e))?;

        env.call_method(&decor_view, "getWindowVisibleDisplayFrame", "(Landroid/graphics/Rect;)V",
            &[JValue::Object(&rect)])
            .map_err(|e| format!("getWindowVisibleDisplayFrame: {}", e))?;

        let visible_bottom = env.get_field(&rect, "bottom", "I")
            .map_err(|e| format!("get rect.bottom: {}", e))?
            .i().map_err(|e| format!("rect.bottom cast: {}", e))?;

        let keyboard_px = (root_height - visible_bottom).max(0);
        let keyboard_dp = keyboard_px as f32 / scale_factor;

        std::mem::forget(vm);
        Ok(keyboard_dp)
    }

    #[cfg(target_os = "android")]
    pub(super) fn set_status_bar_light_icons(&self, light_icons: bool) {
        let Some(ref android_app) = self.android_app else { return };
        let vm_ptr = android_app.vm_as_ptr();
        let activity_ptr = android_app.activity_as_ptr();
        if let Err(e) = unsafe { Self::set_status_bar_appearance_jni(vm_ptr, activity_ptr, light_icons) } {
            log::warn!("Failed to set status bar appearance: {}", e);
        }
    }

    #[cfg(target_os = "android")]
    unsafe fn set_status_bar_appearance_jni(
        vm_ptr: *mut std::ffi::c_void,
        activity_ptr: *mut std::ffi::c_void,
        light_icons: bool,
    ) -> Result<(), String> {
        use jni::objects::{JObject, JValue};

        let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
            .map_err(|e| format!("JavaVM::from_raw: {}", e))?;
        let mut env = vm.attach_current_thread_permanently()
            .map_err(|e| format!("attach_thread: {}", e))?;

        let activity = JObject::from_raw(activity_ptr as jni::sys::jobject);

        let window = env.call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])
            .map_err(|e| format!("getWindow: {}", e))?
            .l().map_err(|e| format!("getWindow cast: {}", e))?;

        let controller = env.call_method(
            &window, "getInsetsController",
            "()Landroid/view/WindowInsetsController;", &[],
        ).map_err(|e| format!("getInsetsController: {}", e))?
            .l().map_err(|e| format!("getInsetsController cast: {}", e))?;

        if controller.is_null() {
            return Err("InsetsController is null".to_string());
        }

        let appearance_flag: i32 = 0x00000008;
        let appearance = if light_icons { 0 } else { appearance_flag };
        env.call_method(
            &controller, "setSystemBarsAppearance", "(II)V",
            &[JValue::Int(appearance), JValue::Int(appearance_flag)],
        ).map_err(|e| format!("setSystemBarsAppearance: {}", e))?;

        std::mem::forget(vm);
        Ok(())
    }
}
