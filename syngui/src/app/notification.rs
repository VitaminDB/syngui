use std::sync::atomic::{AtomicPtr, Ordering};

static VM_PTR: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static NOTIF_CLASS: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

pub(crate) fn set_jni_ptrs(vm: *mut std::ffi::c_void, class_ref: *mut ()) {
    VM_PTR.store(vm as *mut (), Ordering::SeqCst);
    NOTIF_CLASS.store(class_ref, Ordering::SeqCst);
}

fn get_ptrs() -> Option<(*mut std::ffi::c_void, *mut ())> {
    let vm = VM_PTR.load(Ordering::SeqCst);
    let cls = NOTIF_CLASS.load(Ordering::SeqCst);
    if vm.is_null() || cls.is_null() {
        None
    } else {
        Some((vm as *mut std::ffi::c_void, cls))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ChannelImportance {
    Min = 1,
    Low = 2,
    Default = 3,
    High = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum NotificationPriority {
    Min = -2,
    Low = -1,
    Default = 0,
    High = 1,
    Max = 2,
}

#[derive(Clone, Debug)]
pub enum ProgressStyle {
    Determinate { max: i32, progress: i32 },
    Indeterminate,
}

#[derive(Clone, Debug)]
pub struct ChronometerStyle {
    pub base_time_ms: i64,
    pub count_down: bool,
}

#[derive(Clone, Debug)]
pub struct NotificationAction {
    pub notification_id: i32,
    pub action_index: usize,
}

pub struct NotificationBuilder {
    id: i32,
    channel_id: String,
    title: String,
    text: String,
    big_text: Option<String>,
    priority: NotificationPriority,
    auto_cancel: bool,
    ongoing: bool,
    actions: Vec<String>,
    progress: Option<ProgressStyle>,
    chronometer: Option<ChronometerStyle>,
}

impl NotificationBuilder {
    pub fn new(id: i32, channel_id: &str, title: &str, text: &str) -> Self {
        Self {
            id,
            channel_id: channel_id.to_string(),
            title: title.to_string(),
            text: text.to_string(),
            big_text: None,
            priority: NotificationPriority::Default,
            auto_cancel: true,
            ongoing: false,
            actions: Vec::new(),
            progress: None,
            chronometer: None,
        }
    }

    pub fn big_text(mut self, text: &str) -> Self {
        self.big_text = Some(text.to_string());
        self
    }

    pub fn priority(mut self, p: NotificationPriority) -> Self {
        self.priority = p;
        self
    }

    pub fn auto_cancel(mut self, v: bool) -> Self {
        self.auto_cancel = v;
        self
    }

    pub fn ongoing(mut self, v: bool) -> Self {
        self.ongoing = v;
        self
    }

    pub fn action(mut self, label: &str) -> Self {
        self.actions.push(label.to_string());
        self
    }

    pub fn progress(mut self, style: ProgressStyle) -> Self {
        self.progress = Some(style);
        self
    }

    pub fn chronometer(mut self, style: ChronometerStyle) -> Self {
        self.chronometer = Some(style);
        self
    }

    pub fn post(self) {
        if let Some(style) = &self.chronometer {
            post_chronometer_jni(self.id, &self.channel_id, &self.title, style.base_time_ms, style.count_down);
        } else if let Some(style) = &self.progress {
            let (max, progress, indeterminate) = match style {
                ProgressStyle::Determinate { max, progress } => (*max, *progress, false),
                ProgressStyle::Indeterminate => (0, 0, true),
            };
            post_progress_jni(self.id, &self.channel_id, &self.title, &self.text, max, progress, indeterminate);
        } else {
            post_notify_jni(
                self.id,
                &self.channel_id,
                &self.title,
                &self.text,
                self.big_text.as_deref(),
                self.priority as i32,
                self.auto_cancel,
                self.ongoing,
                &self.actions,
            );
        }
    }
}

pub fn create_channel(id: &str, name: &str, description: &str, importance: ChannelImportance) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { create_channel_jni(vm, cls, id, name, description, importance as i32) };
}

pub fn delete_channel(id: &str) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { delete_channel_jni(vm, cls, id) };
}

pub fn cancel(id: i32) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { cancel_jni(vm, cls, id) };
}

pub fn cancel_all() {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { cancel_all_jni(vm, cls) };
}

pub fn has_permission() -> bool {
    let Some((vm, cls)) = get_ptrs() else { return false };
    unsafe { has_permission_jni(vm, cls) }
}

pub fn request_permission() {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { request_permission_jni(vm, cls) };
}

pub fn poll_action() -> Option<NotificationAction> {
    let Some((vm, cls)) = get_ptrs() else { return None };
    unsafe { poll_action_jni(vm, cls) }
}

pub fn schedule_alarm(alarm_id: i32, delay_secs: u64, channel_id: &str, title: &str, text: &str) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { schedule_alarm_jni(vm, cls, alarm_id, delay_secs as i32, channel_id, title, text) };
}

pub fn cancel_alarm(alarm_id: i32) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { cancel_alarm_jni(vm, cls, alarm_id) };
}

/// Запустить/обновить foreground-сервис живого прогресс-бара обратного отсчёта.
///
/// `start_ms`..`deadline_ms` — окно отсчёта (epoch ms); сервис сам обновляет бар
/// раз в секунду и переживает сворачивание/выгрузку процесса. `wait_fmt` —
/// формат подписи ожидания с одним `%s` (подставляется ЧЧ:ММ:СС).
pub fn start_foreground_timer(
    channel_id: &str,
    title: &str,
    start_ms: i64,
    deadline_ms: i64,
    ready_text: &str,
    wait_fmt: &str,
) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe {
        let _ = start_foreground_timer_jni(
            vm, cls, channel_id, title, start_ms, deadline_ms, ready_text, wait_fmt,
        );
    }
}

/// Остановить foreground-сервис прогресс-бара.
pub fn stop_foreground_timer() {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe { stop_foreground_timer_jni(vm, cls) };
}

fn post_notify_jni(
    id: i32, channel_id: &str, title: &str, text: &str,
    big_text: Option<&str>, priority: i32, auto_cancel: bool, ongoing: bool,
    actions: &[String],
) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe {
        let _ = post_notify_jni_inner(vm, cls, id, channel_id, title, text, big_text, priority, auto_cancel, ongoing, actions);
    }
}

fn post_progress_jni(id: i32, channel_id: &str, title: &str, text: &str, max: i32, progress: i32, indeterminate: bool) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe {
        let _ = post_progress_jni_inner(vm, cls, id, channel_id, title, text, max, progress, indeterminate);
    }
}

fn post_chronometer_jni(id: i32, channel_id: &str, title: &str, when_ms: i64, count_down: bool) {
    let Some((vm, cls)) = get_ptrs() else { return };
    unsafe {
        let _ = post_chronometer_jni_inner(vm, cls, id, channel_id, title, when_ms, count_down);
    }
}

unsafe fn with_env<F, R>(vm_ptr: *mut std::ffi::c_void, f: F) -> Result<R, String>
where
    F: FnOnce(&mut jni::JNIEnv) -> Result<R, String>,
{
    let vm = jni::JavaVM::from_raw(vm_ptr as *mut jni::sys::JavaVM)
        .map_err(|e| format!("JavaVM: {e}"))?;
    let mut env = vm.attach_current_thread_permanently()
        .map_err(|e| format!("attach: {e}"))?;
    let result = f(&mut env);
    std::mem::forget(vm);
    result
}

macro_rules! jclass {
    ($ptr:expr) => {
        jni::objects::JClass::from_raw($ptr as jni::sys::jclass)
    };
}

unsafe fn create_channel_jni(
    vm: *mut std::ffi::c_void, cls: *mut (),
    id: &str, name: &str, desc: &str, importance: i32,
) {
    let _ = with_env(vm, |env| {
        let j_id = env.new_string(id).map_err(|e| format!("{e}"))?;
        let j_name = env.new_string(name).map_err(|e| format!("{e}"))?;
        let j_desc = env.new_string(desc).map_err(|e| format!("{e}"))?;
        let cls = jclass!(cls);
        env.call_static_method(
            &cls, "createChannel",
            "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)V",
            &[
                jni::objects::JValue::Object(&j_id),
                jni::objects::JValue::Object(&j_name),
                jni::objects::JValue::Object(&j_desc),
                jni::objects::JValue::Int(importance),
            ],
        ).map_err(|e| format!("createChannel: {e}"))?;
        Ok(())
    });
}

unsafe fn delete_channel_jni(vm: *mut std::ffi::c_void, cls: *mut (), id: &str) {
    let _ = with_env(vm, |env| {
        let j_id = env.new_string(id).map_err(|e| format!("{e}"))?;
        let cls = jclass!(cls);
        env.call_static_method(
            &cls, "deleteChannel", "(Ljava/lang/String;)V",
            &[jni::objects::JValue::Object(&j_id)],
        ).map_err(|e| format!("deleteChannel: {e}"))?;
        Ok(())
    });
}

unsafe fn cancel_jni(vm: *mut std::ffi::c_void, cls: *mut (), id: i32) {
    let _ = with_env(vm, |env| {
        let cls = jclass!(cls);
        env.call_static_method(
            &cls, "cancel", "(I)V",
            &[jni::objects::JValue::Int(id)],
        ).map_err(|e| format!("cancel: {e}"))?;
        Ok(())
    });
}

unsafe fn cancel_all_jni(vm: *mut std::ffi::c_void, cls: *mut ()) {
    let _ = with_env(vm, |env| {
        let cls = jclass!(cls);
        env.call_static_method(&cls, "cancelAll", "()V", &[])
            .map_err(|e| format!("cancelAll: {e}"))?;
        Ok(())
    });
}

unsafe fn has_permission_jni(vm: *mut std::ffi::c_void, cls: *mut ()) -> bool {
    with_env(vm, |env| {
        let cls = jclass!(cls);
        let result = env.call_static_method(&cls, "hasPermission", "()Z", &[])
            .map_err(|e| format!("hasPermission: {e}"))?
            .z().map_err(|e| format!("cast: {e}"))?;
        Ok(result)
    }).unwrap_or(false)
}

unsafe fn request_permission_jni(vm: *mut std::ffi::c_void, cls: *mut ()) {
    let _ = with_env(vm, |env| {
        let cls = jclass!(cls);
        env.call_static_method(&cls, "requestPermission", "()V", &[])
            .map_err(|e| format!("requestPermission: {e}"))?;
        Ok(())
    });
}

unsafe fn poll_action_jni(vm: *mut std::ffi::c_void, cls: *mut ()) -> Option<NotificationAction> {
    with_env(vm, |env| {
        let cls = jclass!(cls);
        let result = env.call_static_method(&cls, "pollAction", "()Ljava/lang/String;", &[])
            .map_err(|e| format!("pollAction: {e}"))?
            .l().map_err(|e| format!("cast: {e}"))?;

        if result.is_null() {
            return Ok(None);
        }

        let jstr = jni::objects::JString::from(result);
        let s: String = env.get_string(&jstr)
            .map_err(|e| format!("getString: {e}"))?
            .into();

        if let Some(rest) = s.strip_prefix("A:") {
            let parts: Vec<&str> = rest.splitn(2, ':').collect();
            if parts.len() == 2 {
                if let (Ok(nid), Ok(aidx)) = (parts[0].parse::<i32>(), parts[1].parse::<usize>()) {
                    return Ok(Some(NotificationAction {
                        notification_id: nid,
                        action_index: aidx,
                    }));
                }
            }
        }
        Ok(None)
    }).unwrap_or(None)
}

unsafe fn post_notify_jni_inner(
    vm: *mut std::ffi::c_void, cls_ptr: *mut (),
    id: i32, channel_id: &str, title: &str, text: &str,
    big_text: Option<&str>, priority: i32, auto_cancel: bool, ongoing: bool,
    actions: &[String],
) -> Result<(), String> {
    with_env(vm, |env| {
        let j_channel = env.new_string(channel_id).map_err(|e| format!("{e}"))?;
        let j_title = env.new_string(title).map_err(|e| format!("{e}"))?;
        let j_text = env.new_string(text).map_err(|e| format!("{e}"))?;
        let j_big: jni::objects::JObject = if let Some(bt) = big_text {
            env.new_string(bt).map_err(|e| format!("{e}"))?.into()
        } else {
            jni::objects::JObject::null()
        };

        let str_cls = env.find_class("java/lang/String").map_err(|e| format!("{e}"))?;
        let action_arr = env.new_object_array(
            actions.len() as i32, &str_cls, &jni::objects::JObject::null(),
        ).map_err(|e| format!("{e}"))?;
        for (i, label) in actions.iter().enumerate() {
            let j_label = env.new_string(label).map_err(|e| format!("{e}"))?;
            env.set_object_array_element(&action_arr, i as i32, j_label)
                .map_err(|e| format!("{e}"))?;
        }

        let cls = jclass!(cls_ptr);
        let j_action_arr = jni::objects::JObject::from_raw(action_arr.as_raw());
        env.call_static_method(
            &cls, "notify",
            "(ILjava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;IZZ[Ljava/lang/String;)V",
            &[
                jni::objects::JValue::Int(id),
                jni::objects::JValue::Object(&j_channel),
                jni::objects::JValue::Object(&j_title),
                jni::objects::JValue::Object(&j_text),
                jni::objects::JValue::Object(&j_big),
                jni::objects::JValue::Int(priority),
                jni::objects::JValue::Bool(auto_cancel as u8),
                jni::objects::JValue::Bool(ongoing as u8),
                jni::objects::JValue::Object(&j_action_arr),
            ],
        ).map_err(|e| format!("notify: {e}"))?;
        Ok(())
    })
}

unsafe fn post_progress_jni_inner(
    vm: *mut std::ffi::c_void, cls_ptr: *mut (),
    id: i32, channel_id: &str, title: &str, text: &str,
    max: i32, progress: i32, indeterminate: bool,
) -> Result<(), String> {
    with_env(vm, |env| {
        let j_channel = env.new_string(channel_id).map_err(|e| format!("{e}"))?;
        let j_title = env.new_string(title).map_err(|e| format!("{e}"))?;
        let j_text = env.new_string(text).map_err(|e| format!("{e}"))?;
        let cls = jclass!(cls_ptr);

        env.call_static_method(
            &cls, "notifyProgress",
            "(ILjava/lang/String;Ljava/lang/String;Ljava/lang/String;IIZ)V",
            &[
                jni::objects::JValue::Int(id),
                jni::objects::JValue::Object(&j_channel),
                jni::objects::JValue::Object(&j_title),
                jni::objects::JValue::Object(&j_text),
                jni::objects::JValue::Int(max),
                jni::objects::JValue::Int(progress),
                jni::objects::JValue::Bool(indeterminate as u8),
            ],
        ).map_err(|e| format!("notifyProgress: {e}"))?;
        Ok(())
    })
}

unsafe fn post_chronometer_jni_inner(
    vm: *mut std::ffi::c_void, cls_ptr: *mut (),
    id: i32, channel_id: &str, title: &str, when_ms: i64, count_down: bool,
) -> Result<(), String> {
    with_env(vm, |env| {
        let j_channel = env.new_string(channel_id).map_err(|e| format!("{e}"))?;
        let j_title = env.new_string(title).map_err(|e| format!("{e}"))?;
        let cls = jclass!(cls_ptr);

        env.call_static_method(
            &cls, "notifyChronometer",
            "(ILjava/lang/String;Ljava/lang/String;JZ)V",
            &[
                jni::objects::JValue::Int(id),
                jni::objects::JValue::Object(&j_channel),
                jni::objects::JValue::Object(&j_title),
                jni::objects::JValue::Long(when_ms),
                jni::objects::JValue::Bool(count_down as u8),
            ],
        ).map_err(|e| format!("notifyChronometer: {e}"))?;
        Ok(())
    })
}

unsafe fn schedule_alarm_jni(
    vm: *mut std::ffi::c_void, cls_ptr: *mut (),
    alarm_id: i32, delay_secs: i32, channel_id: &str, title: &str, text: &str,
) {
    let _ = with_env(vm, |env| {
        let j_channel = env.new_string(channel_id).map_err(|e| format!("{e}"))?;
        let j_title = env.new_string(title).map_err(|e| format!("{e}"))?;
        let j_text = env.new_string(text).map_err(|e| format!("{e}"))?;
        let cls = jclass!(cls_ptr);

        env.call_static_method(
            &cls, "scheduleAlarm",
            "(IILjava/lang/String;Ljava/lang/String;Ljava/lang/String;)V",
            &[
                jni::objects::JValue::Int(alarm_id),
                jni::objects::JValue::Int(delay_secs),
                jni::objects::JValue::Object(&j_channel),
                jni::objects::JValue::Object(&j_title),
                jni::objects::JValue::Object(&j_text),
            ],
        ).map_err(|e| format!("scheduleAlarm: {e}"))?;
        Ok(())
    });
}

unsafe fn cancel_alarm_jni(vm: *mut std::ffi::c_void, cls_ptr: *mut (), alarm_id: i32) {
    let _ = with_env(vm, |env| {
        let cls = jclass!(cls_ptr);
        env.call_static_method(
            &cls, "cancelAlarm", "(I)V",
            &[jni::objects::JValue::Int(alarm_id)],
        ).map_err(|e| format!("cancelAlarm: {e}"))?;
        Ok(())
    });
}

#[allow(clippy::too_many_arguments)]
unsafe fn start_foreground_timer_jni(
    vm: *mut std::ffi::c_void, cls_ptr: *mut (),
    channel_id: &str, title: &str, start_ms: i64, deadline_ms: i64,
    ready_text: &str, wait_fmt: &str,
) -> Result<(), String> {
    with_env(vm, |env| {
        let j_channel = env.new_string(channel_id).map_err(|e| format!("{e}"))?;
        let j_title = env.new_string(title).map_err(|e| format!("{e}"))?;
        let j_ready = env.new_string(ready_text).map_err(|e| format!("{e}"))?;
        let j_fmt = env.new_string(wait_fmt).map_err(|e| format!("{e}"))?;
        let cls = jclass!(cls_ptr);
        env.call_static_method(
            &cls, "startForegroundTimer",
            "(Ljava/lang/String;Ljava/lang/String;JJLjava/lang/String;Ljava/lang/String;)V",
            &[
                jni::objects::JValue::Object(&j_channel),
                jni::objects::JValue::Object(&j_title),
                jni::objects::JValue::Long(start_ms),
                jni::objects::JValue::Long(deadline_ms),
                jni::objects::JValue::Object(&j_ready),
                jni::objects::JValue::Object(&j_fmt),
            ],
        ).map_err(|e| format!("startForegroundTimer: {e}"))?;
        Ok(())
    })
}

unsafe fn stop_foreground_timer_jni(vm: *mut std::ffi::c_void, cls_ptr: *mut ()) {
    let _ = with_env(vm, |env| {
        let cls = jclass!(cls_ptr);
        env.call_static_method(&cls, "stopForegroundTimer", "()V", &[])
            .map_err(|e| format!("stopForegroundTimer: {e}"))?;
        Ok(())
    });
}
