use crate::core::Rect;
use crate::widget::ElementId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct A11yId(pub u64);

impl A11yId {
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 62);
        A11yId(COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }

    /// Стабильный id узла элемента: тот же между `sync`, иначе скринридер
    /// терял фокус и позицию на каждом обновлении дерева.
    pub fn for_element(id: ElementId) -> Self {
        A11yId(id.0)
    }

    /// Синтетический корень, когда верхних узлов несколько.
    pub const SYNTHETIC_ROOT: A11yId = A11yId(u64::MAX);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Document,
    Application,
    Group,

    Button,
    CheckBox,
    RadioButton,
    TextField,
    Slider,
    ScrollBar,
    ProgressBar,

    ComboBox,
    ListBox,
    Menu,
    MenuBar,
    Tree,
    TabList,

    StaticText,
    Heading(u8),
    Paragraph,

    Link,
    Image,

    Terminal,

    None,
    Presentation,
}

#[derive(Clone, Debug, Default)]
pub struct NodeState {
    pub disabled: bool,
    pub focused: bool,
    pub hidden: bool,
    pub pressed: bool,
    pub checked: Option<bool>,
    pub selected: bool,
    pub expanded: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveRegion {
    Off,
    Polite,
    Assertive,
}

impl Default for LiveRegion {
    fn default() -> Self {
        LiveRegion::Off
    }
}

#[derive(Clone, Debug, Default)]
pub struct NodeProperties {
    pub label: Option<String>,
    pub description: Option<String>,
    pub value: Option<String>,
    pub placeholder: Option<String>,
    pub keyboard_shortcut: Option<String>,
    pub live_region: Option<LiveRegion>,
}

#[derive(Clone, Debug)]
pub struct AccessibilityInfo {
    pub role: Role,
    pub state: NodeState,
    pub properties: NodeProperties,
}

#[derive(Clone, Debug)]
pub struct A11yNode {
    pub id: A11yId,
    pub role: Role,
    pub state: NodeState,
    pub properties: NodeProperties,
    pub parent: Option<A11yId>,
    pub children: Vec<A11yId>,
    pub element_id: ElementId,
    pub bounds: Rect,
}

#[derive(Clone, Debug)]
pub enum Action {
    Click,
    Focus,
    SetValue(String),
    Increment,
    Decrement,
    Expand,
    Collapse,
}
