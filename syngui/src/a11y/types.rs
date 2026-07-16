use crate::core::Rect;
use crate::widget::ElementId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct A11yId(pub u64);

impl A11yId {
    pub fn new() -> Self {
        static mut COUNTER: u64 = 0;
        unsafe {
            COUNTER += 1;
            A11yId(COUNTER)
        }
    }
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
