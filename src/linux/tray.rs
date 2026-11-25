//! The panel icon: a StatusNotifierItem with a com.canonical.dbusmenu menu, both
//! over GIO's D-Bus (GTK3 has no tray of its own). KDE and most panels show it;
//! GNOME needs the AppIndicator extension, as with the Qt build.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::gio;
use gtk::gdk_pixbuf::Pixbuf;
use gtk::glib::{self, ToVariant, Variant};

use super::utils;

const ITEM: &str = "/StatusNotifierItem";
const MENU: &str = "/MenuBar";
const XML: &str = r#"<node>
<interface name="org.kde.StatusNotifierItem">
  <property name="Category" type="s" access="read"/>
  <property name="Id" type="s" access="read"/>
  <property name="Title" type="s" access="read"/>
  <property name="Status" type="s" access="read"/>
  <property name="IconName" type="s" access="read"/>
  <property name="IconPixmap" type="a(iiay)" access="read"/>
  <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
  <property name="ItemIsMenu" type="b" access="read"/>
  <property name="Menu" type="o" access="read"/>
  <method name="Activate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <method name="SecondaryActivate"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <method name="ContextMenu"><arg type="i" direction="in"/><arg type="i" direction="in"/></method>
  <method name="Scroll"><arg type="i" direction="in"/><arg type="s" direction="in"/></method>
  <signal name="NewIcon"/>
</interface>
<interface name="com.canonical.dbusmenu">
  <property name="Version" type="u" access="read"/>
  <property name="TextDirection" type="s" access="read"/>
  <property name="Status" type="s" access="read"/>
  <property name="IconThemePath" type="as" access="read"/>
  <method name="GetLayout">
    <arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/>
    <arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/>
  </method>
  <method name="GetGroupProperties">
    <arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/>
  </method>
  <method name="GetProperty">
    <arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/>
  </method>
  <method name="Event">
    <arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/>
  </method>
  <method name="EventGroup">
    <arg type="a(isvu)" direction="in"/><arg type="ai" direction="out"/>
  </method>
  <method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
  <method name="AboutToShowGroup">
    <arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/>
  </method>
  <signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
  <signal name="ItemsPropertiesUpdated"><arg type="a(ia{sv})"/><arg type="a(ia{sv})"/></signal>
</interface>
</node>"#;

/// Menu ids: 0 is the root, an empty label is a separator.
const ITEMS: [(i32, &str); 9] = [
    (1, ""), // Activate / Deactivate, filled in by label()
    (2, "-"),
    (3, "Settings…"),
    (4, "-"),
    (5, "About…"),
    (6, "Check for Updates…"),
    (7, "Setup Guide…"),
    (8, "Help…"),
    (9, "Quit"),
];

struct Tray {
    connection: Option<gio::DBusConnection>,
    pixmap: Vec<(i32, i32, Vec<u8>)>,
    toggle: &'static str,
    revision: u32,
    clicked: Option<Box<dyn Fn(i32)>>,
}

thread_local! {
    static TRAY: RefCell<Tray> = RefCell::new(Tray {
        connection: None,
        pixmap: Vec::new(),
        toggle: "Activate",
        revision: 1,
        clicked: None,
    });
}

/// Shows the icon. `clicked` gets the menu id: 1 toggle, 3 settings, 5 about,
/// 6 updates, 7 guide, 8 help, 9 quit.
pub fn start(state: &str, clicked: impl Fn(i32) + 'static) {
    TRAY.with(|tray| tray.borrow_mut().clicked = Some(Box::new(clicked)));
    set_state(state);
    let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    gio::bus_own_name(
        gio::BusType::Session,
        &name,
        gio::BusNameOwnerFlags::NONE,
        |connection, _| register(&connection),
        |connection, name| {
            let _ = connection.call_sync(
                Some("org.kde.StatusNotifierWatcher"),
                "/StatusNotifierWatcher",
                "org.kde.StatusNotifierWatcher",
                "RegisterStatusNotifierItem",
                Some(&(name,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
            );
        },
        |_, _| (),
    );
}

fn register(connection: &gio::DBusConnection) {
    let node = gio::DBusNodeInfo::for_xml(XML).expect("tray interfaces");
    let item = node.lookup_interface("org.kde.StatusNotifierItem").unwrap();
    let menu = node.lookup_interface("com.canonical.dbusmenu").unwrap();
    // GIO calls these on this (the main) thread, so they reach TRAY
    let _ = connection.register_object(ITEM, &item, |_, _, _, _, _, _, invocation| invocation.return_value(None), item_property, |_, _, _, _, _, _| false);
    let _ = connection.register_object(MENU, &menu, menu_call, menu_property, |_, _, _, _, _, _| false);
    TRAY.with(|tray| tray.borrow_mut().connection = Some(connection.clone()));
}

fn item_property(_: gio::DBusConnection, _: &str, _: &str, _: &str, name: &str) -> Variant {
    TRAY.with(|tray| {
        let tray = tray.borrow();
        match name {
            "Category" => "ApplicationStatus".to_variant(),
            "Id" | "Title" => "unplugg".to_variant(),
            "Status" => "Active".to_variant(),
            "IconName" => "".to_variant(),
            "IconPixmap" => tray.pixmap.to_variant(),
            "ToolTip" => ("", Vec::<(i32, i32, Vec<u8>)>::new(), "unplugg", "").to_variant(),
            "ItemIsMenu" => true.to_variant(),
            _ => glib::variant::ObjectPath::try_from(MENU).unwrap().to_variant(),
        }
    })
}

fn menu_property(_: gio::DBusConnection, _: &str, _: &str, _: &str, name: &str) -> Variant {
    match name {
        "Version" => 3u32.to_variant(),
        "TextDirection" => "ltr".to_variant(),
        "Status" => "normal".to_variant(),
        _ => Vec::<String>::new().to_variant(),
    }
}

fn properties(id: i32, toggle: &str) -> HashMap<String, Variant> {
    let mut props = HashMap::new();
    let label = ITEMS.iter().find(|(i, _)| *i == id).map_or("", |(_, label)| *label);
    match (id, label) {
        (0, _) => {
            props.insert("children-display".into(), "submenu".to_variant());
        }
        (_, "-") => {
            props.insert("type".into(), "separator".to_variant());
        }
        (1, _) => {
            props.insert("label".into(), toggle.to_variant());
        }
        _ => {
            props.insert("label".into(), label.to_variant());
        }
    }
    props
}

fn layout(toggle: &str) -> Variant {
    let children: Vec<Variant> = ITEMS
        .iter()
        .map(|(id, _)| (*id, properties(*id, toggle), Vec::<Variant>::new()).to_variant().to_variant())
        .collect();
    (0i32, properties(0, toggle), children).to_variant()
}

fn menu_call(_: gio::DBusConnection, _: &str, _: &str, _: &str, method: &str, args: Variant, invocation: gio::DBusMethodInvocation) {
    let (revision, toggle) = TRAY.with(|tray| {
        let tray = tray.borrow();
        (tray.revision, tray.toggle)
    });
    match method {
        "GetLayout" => {
            let reply = Variant::tuple_from_iter([revision.to_variant(), layout(toggle)]);
            invocation.return_value(Some(&reply));
        }
        "GetGroupProperties" => {
            let ids: Vec<i32> = args.child_value(0).get().unwrap_or_default();
            let groups: Vec<(i32, HashMap<String, Variant>)> = ids.into_iter().map(|id| (id, properties(id, toggle))).collect();
            invocation.return_value(Some(&(groups,).to_variant()));
        }
        "GetProperty" => {
            let id: i32 = args.child_value(0).get().unwrap_or_default();
            let name: String = args.child_value(1).get().unwrap_or_default();
            let value = properties(id, toggle).remove(&name).unwrap_or_else(|| "".to_variant());
            invocation.return_value(Some(&(value,).to_variant()));
        }
        "Event" => {
            invocation.return_value(None);
            let id: i32 = args.child_value(0).get().unwrap_or_default();
            let event: String = args.child_value(1).get().unwrap_or_default();
            if event == "clicked" {
                // after the reply, as a dialog may block the loop
                glib::idle_add_local_once(move || click(id));
            }
        }
        "EventGroup" => invocation.return_value(Some(&(Vec::<i32>::new(),).to_variant())),
        "AboutToShow" => invocation.return_value(Some(&(false,).to_variant())),
        "AboutToShowGroup" => invocation.return_value(Some(&(Vec::<i32>::new(), Vec::<i32>::new()).to_variant())),
        _ => invocation.return_value(None),
    }
}

fn click(id: i32) {
    let clicked = TRAY.with(|tray| tray.borrow_mut().clicked.take());
    if let Some(clicked) = clicked {
        clicked(id);
        TRAY.with(|tray| tray.borrow_mut().clicked = Some(clicked));
    }
}

/// Activate or Deactivate, as the first menu item.
pub fn set_toggle(label: &'static str) {
    let connection = TRAY.with(|tray| {
        let mut tray = tray.borrow_mut();
        tray.toggle = label;
        tray.revision += 1;
        tray.connection.clone().map(|c| (c, tray.revision))
    });
    if let Some((connection, revision)) = connection {
        let _ = connection.emit_signal(None, MENU, "com.canonical.dbusmenu", "LayoutUpdated", Some(&(revision, 0i32).to_variant()));
    }
}

/// idle, armed or alarm: the menu bar glyph, tinted to read on the panel; the alarm stays red.
pub fn set_state(state: &str) {
    let name = match state {
        "armed" => "menubar_armed.svg",
        "alarm" => "menubar_alarm.svg",
        _ => "menubar.svg",
    };
    let Ok(pixbuf) = Pixbuf::from_file_at_scale(utils::asset_path(name), 64, 64, true) else { return };
    let pixbuf = pixbuf.add_alpha(false, 0, 0, 0).unwrap_or(pixbuf);
    let tint = (state != "alarm").then(|| if utils::light_bar() { 0 } else { 255 });
    let (width, height, stride) = (pixbuf.width(), pixbuf.height(), pixbuf.rowstride() as usize);
    let bytes = pixbuf.read_pixel_bytes();
    // RGBA rows to ARGB in network byte order, as the spec asks
    let mut argb = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height as usize {
        for pixel in bytes[y * stride..][..width as usize * 4].chunks_exact(4) {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let (r, g, b) = tint.map_or((r, g, b), |t| (t, t, t));
            argb.extend([a, r, g, b]);
        }
    }
    let connection = TRAY.with(|tray| {
        let mut tray = tray.borrow_mut();
        tray.pixmap = vec![(width, height, argb)];
        tray.connection.clone()
    });
    if let Some(connection) = connection {
        let _ = connection.emit_signal(None, ITEM, "org.kde.StatusNotifierItem", "NewIcon", None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_match_the_interfaces() {
        let layout = Variant::tuple_from_iter([1u32.to_variant(), layout("Activate")]);
        assert_eq!(layout.type_().as_str(), "(u(ia{sv}av))");
        assert_eq!(vec![(1i32, 1i32, vec![0u8; 4])].to_variant().type_().as_str(), "a(iiay)");
        assert_eq!(("", Vec::<(i32, i32, Vec<u8>)>::new(), "unplugg", "").to_variant().type_().as_str(), "(sa(iiay)ss)");
        let groups: Vec<(i32, HashMap<String, Variant>)> = vec![(1, properties(1, "Activate"))];
        assert_eq!((groups,).to_variant().type_().as_str(), "(a(ia{sv}))");
        assert!(gio::DBusNodeInfo::for_xml(XML).is_ok());
    }
}
