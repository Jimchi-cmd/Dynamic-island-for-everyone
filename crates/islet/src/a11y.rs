//! Screen readers. The island is exposed as one group (what it shows right
//! now, e.g. "Now playing: …") with a button for each control on it.
//!
//! - Linux: AccessKit over AT-SPI, created only while a screen reader is on
//!   (read from the desktop's `org.a11y.Status`), so an idle island keeps
//!   its zero wakeups.
//! - Windows: AccessKit over UI Automation; it does nothing until a screen
//!   reader asks.
//! - macOS: the island view answers VoiceOver itself (platform/macos.rs).

use crate::app::A11y;
use crate::platform::Waker;
use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate,
};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

const ROOT: NodeId = NodeId(1);
const ISLAND: NodeId = NodeId(2);
const FIRST_BUTTON: u64 = 10;

fn rect(r: islet_core::geometry::Rect, scale: f64) -> Rect {
    Rect { x0: r.x * scale, y0: r.y * scale, x1: (r.x + r.w) * scale, y1: (r.y + r.h) * scale }
}

/// The accessibility tree for the island as it is now.
pub fn tree(a: &A11y, scale: f64) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label("Islet");
    root.set_children(vec![ISLAND]);
    let mut island = Node::new(Role::Group);
    island.set_label(a.label.clone());
    island.set_bounds(rect(a.bounds, scale));
    island.set_expanded(a.expanded);
    island.add_action(Action::Click);
    let ids: Vec<NodeId> = (0..a.buttons.len() as u64).map(|i| NodeId(FIRST_BUTTON + i)).collect();
    island.set_children(ids.clone());
    let mut nodes = vec![(ROOT, root), (ISLAND, island)];
    for ((name, r), id) in a.buttons.iter().zip(ids) {
        let mut b = Node::new(Role::Button);
        b.set_label(name.clone());
        b.set_bounds(rect(*r, scale));
        b.add_action(Action::Click);
        nodes.push((id, b));
    }
    TreeUpdate { nodes, tree: Some(TreeInfo::new(ROOT)), tree_id: TreeId::ROOT, focus: ROOT }
}

/// Which button a click on `node` means: None for the island itself.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn target(node: NodeId) -> Option<Option<usize>> {
    match node {
        ISLAND => Some(None),
        NodeId(n) if n >= FIRST_BUTTON => Some(Some((n - FIRST_BUTTON) as usize)),
        _ => None,
    }
}

/// The latest tree, for adapters that ask for it from their own thread.
type Latest = Arc<Mutex<Option<TreeUpdate>>>;

#[cfg_attr(target_os = "macos", allow(dead_code))]
struct Activation(Latest);

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.lock().unwrap().clone()
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
struct Actions {
    tx: Sender<Option<usize>>,
    waker: Waker,
}

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        if request.action != Action::Click {
            return;
        }
        if let Some(t) = target(request.target_node) {
            let _ = self.tx.send(t);
            self.waker.wake();
        }
    }
}

#[cfg(target_os = "linux")]
struct Deactivation;

#[cfg(target_os = "linux")]
impl accesskit::DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

pub struct Bridge {
    latest: Latest,
    last: Option<(A11y, f64)>,
    // Linux creates its adapter later (when a screen reader starts).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    tx: Sender<Option<usize>>,
    rx: Receiver<Option<usize>>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    waker: Waker,
    #[cfg(target_os = "linux")]
    adapter: Option<accesskit_unix::Adapter>,
    #[cfg(target_os = "linux")]
    screen_reader: Receiver<bool>,
    #[cfg(target_os = "windows")]
    adapter: Option<accesskit_windows::SubclassingAdapter>,
}

impl Bridge {
    pub fn new(native_handle: isize, waker: Waker) -> Self {
        let (tx, rx) = channel();
        let latest: Latest = Arc::default();
        let _ = native_handle;
        Self {
            #[cfg(target_os = "windows")]
            adapter: Some(accesskit_windows::SubclassingAdapter::new(
                windows::Win32::Foundation::HWND(native_handle as *mut std::ffi::c_void),
                Activation(latest.clone()),
                Actions { tx: tx.clone(), waker: waker.clone() },
            )),
            #[cfg(target_os = "linux")]
            adapter: None,
            #[cfg(target_os = "linux")]
            screen_reader: linux::watch(waker.clone()),
            latest,
            last: None,
            tx,
            rx,
            waker,
        }
    }

    /// Whether anyone is listening (so the island can skip describing itself).
    pub fn active(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.adapter.is_some();
        #[cfg(not(target_os = "linux"))]
        true
    }

    /// Keep the screen reader's view current (cheap when nothing changed).
    pub fn update(&mut self, a: &A11y, origin: (f64, f64), scale: f64) {
        #[cfg(target_os = "linux")]
        while let Ok(on) = self.screen_reader.try_recv() {
            self.adapter = on.then(|| {
                accesskit_unix::Adapter::new(
                    Activation(self.latest.clone()),
                    Actions { tx: self.tx.clone(), waker: self.waker.clone() },
                    Deactivation,
                )
            });
            self.last = None;
        }
        if self.last.as_ref().is_some_and(|(l, s)| l == a && *s == scale) {
            return;
        }
        let update = tree(a, scale);
        *self.latest.lock().unwrap() = Some(update.clone());
        self.last = Some((a.clone(), scale));
        let _ = origin;
        #[cfg(target_os = "linux")]
        if let Some(adapter) = &mut self.adapter {
            let (w, h) = (a.bounds.x + a.bounds.w, a.bounds.y + a.bounds.h);
            let outer = Rect { x0: origin.0, y0: origin.1, x1: origin.0 + w * scale, y1: origin.1 + h * scale };
            adapter.set_root_window_bounds(outer, outer);
            adapter.update_if_active(|| update);
        }
        #[cfg(target_os = "windows")]
        if let Some(adapter) = &mut self.adapter {
            if let Some(events) = adapter.update_if_active(|| update) {
                events.raise();
            }
        }
        #[cfg(target_os = "macos")]
        let _ = update;
    }

    /// Presses a screen reader asked for since the last call.
    pub fn take_presses(&mut self) -> Vec<Option<usize>> {
        self.rx.try_iter().collect()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    //! Is a screen reader running? `org.a11y.Status` on the session bus says,
    //! and signals changes; read with our own wakeup-free connection.
    use crate::platform::Waker;
    use crate::providers::Bus;
    use std::sync::mpsc::{channel, Receiver};
    use zbus::blocking::{fdo::DBusProxy, MessageIterator};
    use zbus::message::Type as MsgType;
    use zbus::MatchRule;

    const PATH: &str = "/org/a11y/bus";
    const IFACE: &str = "org.a11y.Status";

    pub fn watch(waker: Waker) -> Receiver<bool> {
        let (tx, rx) = channel();
        let _ = std::thread::Builder::new().name("islet-a11y".into()).spawn(move || {
            let Ok(conn) = Bus::Session.connect() else { return };
            let read = |name: &str| -> bool {
                conn.call_method(
                    Some("org.a11y.Bus"),
                    PATH,
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &(IFACE, name),
                )
                .ok()
                .and_then(|m| m.body().deserialize::<zbus::zvariant::OwnedValue>().ok())
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false)
            };
            let mut on = read("ScreenReaderEnabled") || read("IsEnabled");
            if on {
                let _ = tx.send(true);
                waker.wake();
            }
            let Ok(dbus) = DBusProxy::new(&conn) else { return };
            let rule = MatchRule::builder()
                .msg_type(MsgType::Signal)
                .path(PATH)
                .and_then(|r| r.interface("org.freedesktop.DBus.Properties"))
                .and_then(|r| r.member("PropertiesChanged"))
                .map(|r| r.build());
            let Ok(rule) = rule else { return };
            if dbus.add_match_rule(rule).is_err() {
                return;
            }
            for msg in MessageIterator::from(&conn) {
                if msg.is_err() {
                    continue;
                }
                let now = read("ScreenReaderEnabled") || read("IsEnabled");
                if now != on {
                    on = now;
                    if tx.send(on).is_err() {
                        return;
                    }
                    waker.wake();
                }
            }
        });
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use islet_core::geometry::Rect as R;

    #[test]
    fn builds_a_tree_with_the_island_and_its_buttons() {
        let a = A11y {
            label: "Now playing: Midnight Signals by The Placeholders".into(),
            expanded: true,
            bounds: R::new(25.0, 0.0, 370.0, 160.0),
            buttons: vec![
                ("Previous".into(), R::new(40.0, 100.0, 40.0, 40.0)),
                ("Pause".into(), R::new(190.0, 100.0, 40.0, 40.0)),
            ],
        };
        let t = tree(&a, 2.0);
        assert_eq!(t.nodes.len(), 4);
        let island = &t.nodes[1].1;
        assert_eq!(island.label(), Some("Now playing: Midnight Signals by The Placeholders"));
        assert_eq!(island.bounds().map(|b| (b.x0, b.x1)), Some((50.0, 790.0)), "physical px");
        assert_eq!(t.nodes[3].1.label(), Some("Pause"));
        assert_eq!(target(NodeId(11)), Some(Some(1)));
        assert_eq!(target(ISLAND), Some(None));
        assert_eq!(target(ROOT), None);
    }
}
