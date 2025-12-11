#[cfg(target_os = "linux")]
use super::rdp_input::client::{RdpInputKeyboard, RdpInputMouse};
use super::*;
use crate::input::*;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::whiteboard;
#[cfg(target_os = "macos")]
use dispatch::Queue;
use hbb_common::{
    get_time,
    message_proto::{pointer_device_event::Union::TouchEvent, touch_event::Union::ScaleUpdate},
    protobuf::EnumOrUnknown,
};
use rdev::{self, EventType, Key as RdevKey, KeyCode, RawKey};
#[cfg(target_os = "macos")]
use rdev::{CGEventSourceStateID, CGEventTapLocation, VirtualInput};
#[cfg(target_os = "linux")]
use scrap::wayland::pipewire::RDP_SESSION_INFO;

// 添加新的导入
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::runtime::Runtime;

use std::{
    convert::TryFrom,
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{self, Instant},
};

const INVALID_CURSOR_POS: i32 = i32::MIN;
const INVALID_DISPLAY_IDX: i32 = -1;

// 定义要发送到本地接口的数据结构
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalInputEvent {
    pub event_type: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub mask: Option<u32>,
    pub buttons: Option<u32>,
    pub evt_type: Option<u32>,
    pub key_event: Option<LocalKeyEventData>,
    pub pointer_event: Option<LocalPointerEventData>,
    pub conn: i32,
    pub timestamp: i64,
    pub username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalKeyEventData {
    pub down: bool,
    pub chr: Option<u32>,
    pub control_key: Option<i32>,
    pub modifiers: Vec<i32>,
    pub mode: i32,
    pub key_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalPointerEventData {
    pub touch_event: Option<LocalTouchEventData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalTouchEventData {
    pub scale: Option<i32>,
}

#[derive(Default)]
struct StateCursor {
    hcursor: u64,
    cursor_data: Arc<Message>,
    cached_cursor_data: HashMap<u64, Arc<Message>>,
}

impl super::service::Reset for StateCursor {
    fn reset(&mut self) {
        *self = Default::default();
        crate::platform::reset_input_cache();
    }
}

struct StatePos {
    cursor_pos: (i32, i32),
}

impl Default for StatePos {
    fn default() -> Self {
        Self {
            cursor_pos: (INVALID_CURSOR_POS, INVALID_CURSOR_POS),
        }
    }
}

impl super::service::Reset for StatePos {
    fn reset(&mut self) {
        self.cursor_pos = (INVALID_CURSOR_POS, INVALID_CURSOR_POS);
    }
}

impl StatePos {
    #[inline]
    fn is_valid(&self) -> bool {
        self.cursor_pos.0 != INVALID_CURSOR_POS
    }

    #[inline]
    fn is_moved(&self, x: i32, y: i32) -> bool {
        self.is_valid() && (self.cursor_pos.0 != x || self.cursor_pos.1 != y)
    }
}

#[derive(Default)]
struct StateWindowFocus {
    display_idx: i32,
}

impl super::service::Reset for StateWindowFocus {
    fn reset(&mut self) {
        self.display_idx = INVALID_DISPLAY_IDX;
    }
}

impl StateWindowFocus {
    #[inline]
    fn is_valid(&self) -> bool {
        self.display_idx != INVALID_DISPLAY_IDX
    }

    #[inline]
    fn is_changed(&self, disp_idx: i32) -> bool {
        self.is_valid() && self.display_idx != disp_idx
    }
}

#[derive(Default, Clone, Copy)]
struct Input {
    conn: i32,
    time: i64,
    x: i32,
    y: i32,
}

#[derive(Clone, Default)]
pub struct MouseCursorSub {
    inner: ConnInner,
    cached: HashMap<u64, Arc<Message>>,
}

impl From<ConnInner> for MouseCursorSub {
    fn from(inner: ConnInner) -> Self {
        Self {
            inner,
            cached: HashMap::new(),
        }
    }
}

impl Subscriber for MouseCursorSub {
    #[inline]
    fn id(&self) -> i32 {
        self.inner.id()
    }

    #[inline]
    fn send(&mut self, msg: Arc<Message>) {
        if let Some(message::Union::CursorData(cd)) = &msg.union {
            if let Some(msg) = self.cached.get(&cd.id) {
                self.inner.send(msg.clone());
            } else {
                self.inner.send(msg.clone());
                let mut tmp = Message::new();
                // only send id out, require client side cache also
                tmp.set_cursor_id(cd.id);
                self.cached.insert(cd.id, Arc::new(tmp));
            }
        } else {
            self.inner.send(msg);
        }
    }
}

pub const NAME_CURSOR: &'static str = "mouse_cursor";
pub const NAME_POS: &'static str = "mouse_pos";
pub const NAME_WINDOW_FOCUS: &'static str = "window_focus";
#[derive(Clone)]
pub struct MouseCursorService {
    pub sp: ServiceTmpl<MouseCursorSub>,
}

impl Deref for MouseCursorService {
    type Target = ServiceTmpl<MouseCursorSub>;

    fn deref(&self) -> &Self::Target {
        &self.sp
    }
}

impl DerefMut for MouseCursorService {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sp
    }
}

impl MouseCursorService {
    pub fn new(name: String, need_snapshot: bool) -> Self {
        Self {
            sp: ServiceTmpl::<MouseCursorSub>::new(name, need_snapshot),
        }
    }
}

pub fn new_cursor() -> ServiceTmpl<MouseCursorSub> {
    let svc = MouseCursorService::new(NAME_CURSOR.to_owned(), true);
    ServiceTmpl::<MouseCursorSub>::repeat::<StateCursor, _, _>(&svc.clone(), 33, run_cursor);
    svc.sp
}

pub fn new_pos() -> GenericService {
    let svc = EmptyExtraFieldService::new(NAME_POS.to_owned(), false);
    GenericService::repeat::<StatePos, _, _>(&svc.clone(), 33, run_pos);
    svc.sp
}

pub fn new_window_focus() -> GenericService {
    let svc = EmptyExtraFieldService::new(NAME_WINDOW_FOCUS.to_owned(), false);
    GenericService::repeat::<StateWindowFocus, _, _>(&svc.clone(), 33, run_window_focus);
    svc.sp
}

#[inline]
fn update_last_cursor_pos(x: i32, y: i32) {
    let mut lock = LATEST_SYS_CURSOR_POS.lock().unwrap();
    if lock.1 .0 != x || lock.1 .1 != y {
        (lock.0, lock.1) = (Some(Instant::now()), (x, y))
    }
}

fn run_pos(sp: EmptyExtraFieldService, state: &mut StatePos) -> ResultType<()> {
    let (_, (x, y)) = *LATEST_SYS_CURSOR_POS.lock().unwrap();
    if x == INVALID_CURSOR_POS || y == INVALID_CURSOR_POS {
        return Ok(());
    }

    if state.is_moved(x, y) {
        let mut msg_out = Message::new();
        msg_out.set_cursor_position(CursorPosition {
            x,
            y,
            ..Default::default()
        });
        let exclude = {
            let now = get_time();
            let lock = LATEST_PEER_INPUT_CURSOR.lock().unwrap();
            if now - lock.time < 300 {
                lock.conn
            } else {
                0
            }
        };
        sp.send_without(msg_out, exclude);
    }
    state.cursor_pos = (x, y);

    sp.snapshot(|sps| {
        let mut msg_out = Message::new();
        msg_out.set_cursor_position(CursorPosition {
            x: state.cursor_pos.0,
            y: state.cursor_pos.1,
            ..Default::default()
        });
        sps.send(msg_out);
        Ok(())
    })?;
    Ok(())
}

fn run_cursor(sp: MouseCursorService, state: &mut StateCursor) -> ResultType<()> {
    if let Some(hcursor) = crate::get_cursor()? {
        if hcursor != state.hcursor {
            let msg;
            if let Some(cached) = state.cached_cursor_data.get(&hcursor) {
                super::log::trace!("Cursor data cached, hcursor: {}", hcursor);
                msg = cached.clone();
            } else {
                let mut data = crate::get_cursor_data(hcursor)?;
                data.colors = hbb_common::compress::compress(&data.colors[..]).into();
                let mut tmp = Message::new();
                tmp.set_cursor_data(data);
                msg = Arc::new(tmp);
                state.cached_cursor_data.insert(hcursor, msg.clone());
                super::log::trace!("Cursor data updated, hcursor: {}", hcursor);
            }
            state.hcursor = hcursor;
            sp.send_shared(msg.clone());
            state.cursor_data = msg;
        }
    }
    sp.snapshot(|sps| {
        sps.send_shared(state.cursor_data.clone());
        Ok(())
    })?;
    Ok(())
}

fn run_window_focus(sp: EmptyExtraFieldService, state: &mut StateWindowFocus) -> ResultType<()> {
    let displays = super::display_service::get_sync_displays();
    if displays.len() <= 1 {
        return Ok(());
    }
    let disp_idx = crate::get_focused_display(displays);
    if let Some(disp_idx) = disp_idx.map(|id| id as i32) {
        if state.is_changed(disp_idx) {
            let mut misc = Misc::new();
            misc.set_follow_current_display(disp_idx as i32);
            let mut msg_out = Message::new();
            msg_out.set_misc(misc);
            sp.send(msg_out);
        }
        state.display_idx = disp_idx;
    }
    Ok(())
}

// 创建全局的HTTP客户端和运行时
lazy_static::lazy_static! {
    static ref LATEST_PEER_INPUT_CURSOR: Arc<Mutex<Input>> = Default::default();
    static ref LATEST_SYS_CURSOR_POS: Arc<Mutex<(Option<Instant>, (i32, i32))>> = Arc::new(Mutex::new((None, (INVALID_CURSOR_POS, INVALID_CURSOR_POS))));
    
    // 本地API相关全局变量
    static ref HTTP_CLIENT: Arc<Mutex<Option<Client>>> = Arc::new(Mutex::new(None));
    static ref LOCAL_API_ENABLED: Arc<Mutex<bool>> = Arc::new(Mutex::new(true)); // 默认启用
    static ref LOCAL_API_URL: Arc<Mutex<String>> = Arc::new(Mutex::new("http://127.0.0.1:8080/event".to_string()));
    static ref TOKIO_RUNTIME: Arc<Mutex<Option<Runtime>>> = Arc::new(Mutex::new(None));
}
static EXITING: AtomicBool = AtomicBool::new(false);

const MOUSE_MOVE_PROTECTION_TIMEOUT: Duration = Duration::from_millis(1_000);
const MOUSE_ACTIVE_DISTANCE: i32 = 5;

static RECORD_CURSOR_POS_RUNNING: AtomicBool = AtomicBool::new(false);

// mac key input must be run in main thread, otherwise crash on >= osx 10.15
#[cfg(target_os = "macos")]
lazy_static::lazy_static! {
    static ref QUEUE: Queue = Queue::main();
}

pub fn try_start_record_cursor_pos() -> Option<thread::JoinHandle<()>> {
    if RECORD_CURSOR_POS_RUNNING.load(Ordering::SeqCst) {
        return None;
    }

    RECORD_CURSOR_POS_RUNNING.store(true, Ordering::SeqCst);
    let handle = thread::spawn(|| {
        let interval = time::Duration::from_millis(33);
        loop {
            if !RECORD_CURSOR_POS_RUNNING.load(Ordering::SeqCst) {
                break;
            }

            let now = time::Instant::now();
            if let Some((x, y)) = crate::get_cursor_pos() {
                update_last_cursor_pos(x, y);
            }
            let elapsed = now.elapsed();
            if elapsed < interval {
                thread::sleep(interval - elapsed);
            }
        }
        update_last_cursor_pos(INVALID_CURSOR_POS, INVALID_CURSOR_POS);
    });
    Some(handle)
}

pub fn try_stop_record_cursor_pos() {
    let remote_count = AUTHED_CONNS
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.conn_type == AuthConnType::Remote)
        .count();
    if remote_count > 0 {
        return;
    }
    RECORD_CURSOR_POS_RUNNING.store(false, Ordering::SeqCst);
}

#[allow(unreachable_code)]
pub fn handle_mouse(
    evt: &MouseEvent,
    conn: i32,
    username: String,
    argb: u32,
    simulate: bool,
    show_cursor: bool,
) {
    #[cfg(target_os = "macos")]
    {
        // having GUI (--server has tray, it is GUI too), run main GUI thread, otherwise crash
        let evt = evt.clone();
        QUEUE.exec_async(move || handle_mouse_(&evt, conn, username, argb, simulate, show_cursor));
        return;
    }
    #[cfg(windows)]
    crate::portable_service::client::handle_mouse(evt, conn, username, argb, simulate, show_cursor);
    #[cfg(not(windows))]
    handle_mouse_(evt, conn, username, argb, simulate, show_cursor);
}

// to-do: merge handle_mouse and handle_pointer
#[allow(unreachable_code)]
pub fn handle_pointer(evt: &PointerDeviceEvent, conn: i32) {
    #[cfg(target_os = "macos")]
    {
        // having GUI, run main GUI thread, otherwise crash
        let evt = evt.clone();
        QUEUE.exec_async(move || handle_pointer_(&evt, conn));
        return;
    }
    #[cfg(windows)]
    crate::portable_service::client::handle_pointer(evt, conn);
    #[cfg(not(windows))]
    handle_pointer_(evt, conn);
}

// check if mouse is moved by the controlled side user to make controlled side has higher mouse priority than remote.
fn active_mouse_(_conn: i32) -> bool {
    true
}

// 初始化本地API
pub async fn init_local_api(url: Option<String>) {
    let mut client_guard = HTTP_CLIENT.lock().await;
    if client_guard.is_none() {
        *client_guard = Some(Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| Client::new()));
    }
    
    let mut enabled_guard = LOCAL_API_ENABLED.lock().await;
    *enabled_guard = true;
    
    if let Some(url) = url {
        let mut url_guard = LOCAL_API_URL.lock().await;
        *url_guard = url;
    }
    
    let mut runtime_guard = TOKIO_RUNTIME.lock().await;
    if runtime_guard.is_none() {
        *runtime_guard = Some(Runtime::new().unwrap());
    }
    
    log::info!("Local API initialized with URL: {}", LOCAL_API_URL.lock().await);
}

// 禁用本地API
pub async fn disable_local_api() {
    let mut enabled_guard = LOCAL_API_ENABLED.lock().await;
    *enabled_guard = false;
    log::info!("Local API disabled");
}

// 发送事件到本地API的辅助函数
async fn send_to_local_api_async(event: LocalInputEvent) {
    let enabled = LOCAL_API_ENABLED.lock().await;
    if !*enabled {
        return;
    }
    let client_guard = HTTP_CLIENT.lock().await;
    let url_guard = LOCAL_API_URL.lock().await;
    
    drop(enabled);
    
    if let Some(client) = client_guard.as_ref() {
        let url = url_guard.clone();
        
        // 异步发送请求
        match client.post(&url).json(&event).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    log::debug!("Failed to send event to local API: {}", response.status());
                }
            }
            Err(e) => {
                log::debug!("Failed to send event to local API: {}", e);
            }
        }
    }
}

// 同步调用发送到本地API
fn send_to_local_api(event: LocalInputEvent) {
    let runtime_guard = TOKIO_RUNTIME.lock();
    if let Ok(mut guard) = runtime_guard {
        if let Some(runtime) = guard.as_mut() {
            runtime.spawn(async move {
                send_to_local_api_async(event).await;
            });
        }
    }
}

// 获取控制键名称
fn get_control_key_name(value: i32) -> String {
    use crate::keyboard::ControlKey::*;
    match value {
        v if v == Alt.value() => "Alt".to_string(),
        v if v == RAlt.value() => "RightAlt".to_string(),
        v if v == Control.value() => "Control".to_string(),
        v if v == RControl.value() => "RightControl".to_string(),
        v if v == Shift.value() => "Shift".to_string(),
        v if v == RShift.value() => "RightShift".to_string(),
        v if v == Meta.value() => "Meta".to_string(),
        v if v == RWin.value() => "RightWin".to_string(),
        v if v == Return.value() => "Return".to_string(),
        v if v == Escape.value() => "Escape".to_string(),
        v if v == Backspace.value() => "Backspace".to_string(),
        v if v == Tab.value() => "Tab".to_string(),
        v if v == Space.value() => "Space".to_string(),
        v if v == CapsLock.value() => "CapsLock".to_string(),
        v if v == NumLock.value() => "NumLock".to_string(),
        v if v == Scroll.value() => "ScrollLock".to_string(),
        v if v == Insert.value() => "Insert".to_string(),
        v if v == Delete.value() => "Delete".to_string(),
        v if v == Home.value() => "Home".to_string(),
        v if v == End.value() => "End".to_string(),
        v if v == PageUp.value() => "PageUp".to_string(),
        v if v == PageDown.value() => "PageDown".to_string(),
        v if v == UpArrow.value() => "UpArrow".to_string(),
        v if v == DownArrow.value() => "DownArrow".to_string(),
        v if v == LeftArrow.value() => "LeftArrow".to_string(),
        v if v == RightArrow.value() => "RightArrow".to_string(),
        v if v == F1.value() => "F1".to_string(),
        v if v == F2.value() => "F2".to_string(),
        v if v == F3.value() => "F3".to_string(),
        v if v == F4.value() => "F4".to_string(),
        v if v == F5.value() => "F5".to_string(),
        v if v == F6.value() => "F6".to_string(),
        v if v == F7.value() => "F7".to_string(),
        v if v == F8.value() => "F8".to_string(),
        v if v == F9.value() => "F9".to_string(),
        v if v == F10.value() => "F10".to_string(),
        v if v == F11.value() => "F11".to_string(),
        v if v == F12.value() => "F12".to_string(),
        _ => format!("Key_{}", value),
    }
}

// 修改handle_pointer_函数，只保留本地API调用
pub fn handle_pointer_(evt: &PointerDeviceEvent, conn: i32) {
    if !active_mouse_(conn) {
        return;
    }

    if EXITING.load(Ordering::SeqCst) {
        return;
    }

    // 发送指针事件到本地API
    let mut pointer_event_data = None;
    
    match &evt.union {
        Some(TouchEvent(touch_evt)) => match &touch_evt.union {
            Some(ScaleUpdate(scale_evt)) => {
                pointer_event_data = Some(LocalPointerEventData {
                    touch_event: Some(LocalTouchEventData {
                        scale: Some(scale_evt.scale),
                    }),
                });
            }
            _ => {
                pointer_event_data = Some(LocalPointerEventData {
                    touch_event: Some(LocalTouchEventData { scale: None }),
                });
            }
        },
        _ => {}
    }
    
    let event = LocalInputEvent {
        event_type: "pointer".to_string(),
        x: None,
        y: None,
        mask: None,
        buttons: None,
        evt_type: None,
        key_event: None,
        pointer_event: pointer_event_data,
        conn,
        timestamp: get_time(),
        username: None,
    };
    
    // 发送到本地API
    send_to_local_api(event);
    
    // 移除了原有的处理逻辑
    log::debug!("Pointer event sent to local API: {:?}", evt);
}

// 修改handle_mouse_simulation_函数，只保留本地API调用
pub fn handle_mouse_simulation_(evt: &MouseEvent, conn: i32) {
    if !active_mouse_(conn) {
        return;
    }

    if EXITING.load(Ordering::SeqCst) {
        return;
    }

    let buttons = evt.mask >> 3;
    let evt_type = evt.mask & 0x7;
    
    // 发送鼠标事件到本地API
    let event = LocalInputEvent {
        event_type: "mouse".to_string(),
        x: Some(evt.x),
        y: Some(evt.y),
        mask: Some(evt.mask),
        buttons: Some(buttons),
        evt_type: Some(evt_type),
        key_event: None,
        pointer_event: None,
        conn,
        timestamp: get_time(),
        username: None,
    };
    
    // 发送到本地API
    send_to_local_api(event);
    
    // 更新最新的光标输入信息（仅用于记录，不进行实际鼠标移动）
    *LATEST_PEER_INPUT_CURSOR.lock().unwrap() = Input {
        conn,
        time: get_time(),
        x: evt.x,
        y: evt.y,
    };
    
    // 移除了原有的鼠标处理逻辑
    log::debug!("Mouse event sent to local API: x={}, y={}, buttons={}, evt_type={}", 
                evt.x, evt.y, buttons, evt_type);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn handle_mouse_show_cursor_(evt: &MouseEvent, conn: i32, username: String, argb: u32) {
    let buttons = evt.mask >> 3;
    let evt_type = evt.mask & 0x7;
    
    // 发送显示光标事件到本地API
    let event = LocalInputEvent {
        event_type: "cursor_show".to_string(),
        x: Some(evt.x),
        y: Some(evt.y),
        mask: Some(evt.mask),
        buttons: Some(buttons),
        evt_type: Some(evt_type),
        key_event: None,
        pointer_event: None,
        conn,
        timestamp: get_time(),
        username: Some(username.clone()),
    };
    
    send_to_local_api(event);
    
    // 移除了原有的白板更新逻辑
    log::debug!("Cursor show event sent to local API: username={}, x={}, y={}", 
                username, evt.x, evt.y);
}

#[inline]
#[cfg(target_os = "linux")]
pub fn handle_key(evt: &KeyEvent) {
    handle_key_(evt);
}

#[inline]
#[cfg(target_os = "windows")]
pub fn handle_key(evt: &KeyEvent) {
    crate::portable_service::client::handle_key(evt);
}

#[inline]
#[cfg(target_os = "macos")]
pub fn handle_key(evt: &KeyEvent) {
    // having GUI, run main GUI thread, otherwise crash
    let evt = evt.clone();
    QUEUE.exec_async(move || handle_key_(&evt));
}

// 修改handle_key_函数，只保留本地API调用
pub fn handle_key_(evt: &KeyEvent) {
    if EXITING.load(Ordering::SeqCst) {
        return;
    }

    // 发送键盘事件到本地API
    let mut key_event_data = None;
    
    match &evt.union {
        Some(key_event::Union::ControlKey(ck)) => {
            key_event_data = Some(LocalKeyEventData {
                down: evt.down,
                chr: None,
                control_key: Some(ck.value()),
                modifiers: evt.modifiers.iter().map(|m| m.value()).collect(),
                mode: evt.mode.value(),
                key_name: Some(get_control_key_name(ck.value())),
            });
        }
        Some(key_event::Union::Chr(chr)) => {
            let key_name = if *chr < 128 {
                Some(format!("Char_{}", *chr as u8 as char))
            } else {
                Some(format!("Char_{}", *chr))
            };
            
            key_event_data = Some(LocalKeyEventData {
                down: evt.down,
                chr: Some(*chr),
                control_key: None,
                modifiers: evt.modifiers.iter().map(|m| m.value()).collect(),
                mode: evt.mode.value(),
                key_name,
            });
        }
        Some(key_event::Union::Unicode(chr)) => {
            let key_name = if let Some(ch) = std::char::from_u32(*chr) {
                Some(format!("Unicode_{}", ch))
            } else {
                Some(format!("Unicode_{}", chr))
            };
            
            key_event_data = Some(LocalKeyEventData {
                down: evt.down,
                chr: Some(*chr),
                control_key: None,
                modifiers: evt.modifiers.iter().map(|m| m.value()).collect(),
                mode: evt.mode.value(),
                key_name,
            });
        }
        Some(key_event::Union::Seq(seq)) => {
            // 对于序列，我们只记录第一个字符
            if let Some(chr) = seq.chars().next() {
                key_event_data = Some(LocalKeyEventData {
                    down: evt.down,
                    chr: Some(chr as u32),
                    control_key: None,
                    modifiers: evt.modifiers.iter().map(|m| m.value()).collect(),
                    mode: evt.mode.value(),
                    key_name: Some(format!("Seq_{}", seq)),
                });
            }
        }
        _ => {}
    }
    
    if let Some(key_data) = &key_event_data {
        let event = LocalInputEvent {
            event_type: "keyboard".to_string(),
            x: None,
            y: None,
            mask: None,
            buttons: None,
            evt_type: None,
            key_event: Some(key_data.clone()),
            pointer_event: None,
            conn: 0, // 键盘事件可能没有conn信息
            timestamp: get_time(),
            username: None,
        };
        
        // 发送到本地API
        send_to_local_api(event);
        
        log::debug!("Key event sent to local API: down={}, key_name={:?}", 
                   evt.down, key_data.key_name);
    } else {
        log::debug!("Unknown key event type: {:?}", evt.union);
    }
    
    // 移除了原有的键盘模式处理逻辑
}

pub async fn lock_screen() {
    // 发送锁屏事件到本地API
    let event = LocalInputEvent {
        event_type: "lock_screen".to_string(),
        x: None,
        y: None,
        mask: None,
        buttons: None,
        evt_type: None,
        key_event: None,
        pointer_event: None,
        conn: 0,
        timestamp: get_time(),
        username: None,
    };
    
    send_to_local_api(event);
    
    log::info!("Lock screen event sent to local API");
}

#[inline]
#[cfg(target_os = "linux")]
pub fn wayland_use_uinput() -> bool {
    !crate::platform::is_x11() && crate::is_server()
}

#[inline]
#[cfg(target_os = "linux")]
pub fn wayland_use_rdp_input() -> bool {
    !crate::platform::is_x11() && !crate::is_server()
}

// 程序初始化时自动启用本地API
pub fn initialize_local_api_on_startup() {
    // 在程序启动时创建一个新的运行时并初始化本地API
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        init_local_api(None).await;
        log::info!("Local API enabled by default on startup");
    });
}

// 添加一个配置函数来启用/禁用本地API
pub async fn configure_local_api(enabled: bool, url: Option<String>) {
    if enabled {
        init_local_api(url).await;
    } else {
        disable_local_api().await;
    }
}

// 添加一个函数来检查本地API状态
pub async fn is_local_api_enabled() -> bool {
    let enabled_guard = LOCAL_API_ENABLED.lock().await;
    *enabled_guard
}

// 在Cargo.toml中添加依赖：
// reqwest = { version = "0.11", features = ["json"] }
// serde = { version = "1.0", features = ["derive"] }
// serde_json = "1.0"
// tokio = { version = "1.0", features = ["full"] }
