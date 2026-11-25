//! Settings window, shaped like the macOS one: General / Alerts / Devices tabs,
//! each a stack of titled groups. The guide builds the same rows. Linux has no
//! per-app camera, mic or location switches, so there's no Permissions tab.

use std::cell::RefCell;
use std::sync::mpsc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::{json, Value};

use super::{app, devices, dialogs, loginitem, sensors, utils};
use crate::telegram::{self, LOUD_HELP, TOKEN_HELP};

thread_local! {
    static WINDOW: RefCell<Option<(gtk::Window, gtk::Notebook)>> = const { RefCell::new(None) };
}

pub fn open_settings() {
    if let Some((window, _)) = WINDOW.with(|w| w.borrow().clone()) {
        window.present();
        return;
    }
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("unplugg Settings");
    window.set_icon(utils::app_icon().as_ref());
    window.set_resizable(false);
    let tabs = gtk::Notebook::new();
    let pages: [(&str, fn(&gtk::Window, &gtk::Box)); 3] = [("General", general_page), ("Alerts", alerts_page), ("Devices", devices_page)];
    for (title, build) in pages {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        column.set_border_width(12);
        build(&window, &column);
        tabs.append_page(&column, Some(&gtk::Label::new(Some(title))));
    }
    window.add(&tabs);
    window.show_all();
    let pane = app::config()["settings_pane"].as_u64().unwrap_or(0).min(tabs.n_pages() as u64 - 1);
    tabs.set_current_page(Some(pane as u32));
    window.connect_delete_event(|_, _| {
        if let Some((_, tabs)) = WINDOW.with(|w| w.borrow_mut().take()) {
            app::update_config("settings_pane", tabs.current_page().unwrap_or(0).into());
        }
        glib::Propagation::Proceed
    });
    window.present();
    WINDOW.with(|w| *w.borrow_mut() = Some((window, tabs)));
}

// ---- pages ------------------------------------------------------------------

fn general_page(_: &gtk::Window, column: &gtk::Box) {
    let mut rows = vec![switch("monitor_power", "Power unplugged"), switch("monitor_lid", "Lid closed")];
    if sensors::acceleration().is_some() {
        rows.push(switch("monitor_motion", "Moved (motion sensor)"));
    }
    group(column, Some("Alarm when"), &rows);
    group(column, Some("While armed"), &[switch("show_overlay", "Show overlay"), switch("lock_device", "Lock when triggered")]);
    group(column, None, &[switch("launch_at_login", "Launch at login")]);
    group(column, Some("Updates"), &[switch("check_updates", "Check for updates automatically"), switch("auto_update", "Install updates automatically")]);
}

fn alerts_page(window: &gtk::Window, column: &gtk::Box) {
    let checks = [
        ("play_sound", "Play siren at full volume"),
        ("capture_media", "Send photo & video"),
        ("flash_screen", "Flash screen for photos"),
        ("record_audio", "Record audio with video"),
        ("send_location", "Send location"),
        ("live_location", "Send live location"),
    ];
    let rows: Vec<gtk::Widget> = checks.iter().map(|(key, text)| switch(key, text)).collect();
    group(column, Some("When triggered"), &rows);
    telegram_group(window, column);
    let folder = gtk::Label::new(Some(&short(app::config()["media_dir"].as_str().unwrap_or(""))));
    folder.set_selectable(true);
    folder.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    folder.set_xalign(0.0);
    let choose = gtk::Button::with_label("Choose…");
    let label = folder.clone();
    let parent = window.clone();
    choose.connect_clicked(move |_| {
        let dialog = gtk::FileChooserDialog::with_buttons(
            Some("Save photos and video in"),
            Some(&parent),
            gtk::FileChooserAction::SelectFolder,
            &[("Cancel", gtk::ResponseType::Cancel), ("Choose", gtk::ResponseType::Accept)],
        );
        let _ = dialog.set_current_folder(app::config()["media_dir"].as_str().unwrap_or(""));
        if dialog.run() == gtk::ResponseType::Accept {
            if let Some(path) = dialog.filename() {
                let path = path.to_string_lossy().into_owned();
                label.set_text(&short(&path));
                app::update_config("media_dir", path.into());
            }
        }
        unsafe { dialog.destroy() };
    });
    let reveal = gtk::Button::with_label("Open");
    reveal.connect_clicked(|_| {
        let dir = app::config()["media_dir"].as_str().unwrap_or("").to_owned();
        let path = dir.strip_prefix('~').map_or_else(|| dir.clone().into(), |rest| crate::config::home().join(rest.trim_start_matches('/')));
        // capture makes it lazily
        let _ = std::fs::create_dir_all(&path);
        utils::open_url(&path.to_string_lossy());
    });
    group(column, Some("Saved media"), &[row(&[folder.upcast(), choose.upcast(), reveal.upcast()], 0)]);
}

pub(super) fn telegram_group(window: &gtk::Window, column: &gtk::Box) {
    let dests = gtk::ComboBoxText::new();
    dests.set_size_request(160, -1);
    let (add, remove, test) = (gtk::Button::with_label("+"), gtk::Button::with_label("−"), gtk::Button::with_label("Send Test"));
    let refresh = {
        let (dests, remove, test) = (dests.clone(), remove.clone(), test.clone());
        move || {
            dests.remove_all();
            let config = app::config();
            let names: Vec<&str> = config["telegram_destinations"].as_array().into_iter().flatten().filter_map(|d| d["name"].as_str()).collect();
            for name in if names.is_empty() { vec!["None yet"] } else { names.clone() } {
                dests.append_text(name);
            }
            dests.set_active(Some(0));
            for widget in [dests.upcast_ref::<gtk::Widget>(), remove.upcast_ref(), test.upcast_ref()] {
                widget.set_sensitive(!names.is_empty());
            }
        }
    };
    refresh();
    let destinations = || app::config()["telegram_destinations"].as_array().cloned().unwrap_or_default();
    let (parent, again) = (window.clone(), refresh.clone());
    add.connect_clicked(move |_| {
        if let Some(dest) = setup_telegram(&parent) {
            let mut all = destinations();
            all.push(dest);
            app::update_config("telegram_destinations", all.into());
            again();
        }
    });
    let (picked, again) = (dests.clone(), refresh.clone());
    remove.connect_clicked(move |_| {
        let mut all = destinations();
        if let Some(index) = picked.active().map(|i| i as usize).filter(|i| *i < all.len()) {
            all.remove(index);
            app::update_config("telegram_destinations", all.into());
            again();
        }
    });
    let (picked, parent) = (dests.clone(), window.clone());
    test.connect_clicked(move |_| {
        let all = destinations();
        let Some(dest) = picked.active().and_then(|i| all.get(i as usize)) else { return };
        let field = |key: &str| dest[key].as_str().map(str::to_owned).unwrap_or_else(|| dest[key].to_string());
        let (title, text) = match telegram::send_test(&field("token"), &field("chat_id")) {
            Ok(()) => ("Test sent", format!("Check Telegram for {}.", field("name"))),
            Err(why) => ("Test failed", format!("Telegram says: {why}")),
        };
        dialogs::alert_with(title, &text, &[], None, Some(&parent));
    });
    let label = gtk::Label::new(Some("Destinations"));
    group(column, Some("Telegram"), &[row(&[label.upcast(), dests.upcast(), add.upcast(), remove.upcast(), test.upcast()], 0)]);
}

fn devices_page(_: &gtk::Window, column: &gtk::Box) {
    let config = app::config();
    let current = |key: &str| config[key].as_str().unwrap_or("").to_owned();
    let or = |items: Vec<String>, default: &str| if items.is_empty() { vec![default.to_owned()] } else { items };
    let cameras = or(devices::cameras(), "Default camera");
    let mics = or(devices::microphones(), "Default microphone");
    let camera = devices::camera(&current("camera")).map(|(_, name)| name);
    let mic = Some(current("mic")).filter(|m| mics.contains(m)).or_else(devices::default_microphone);
    let capture = gtk::Grid::new();
    form(&capture, 0, "Camera", &picker(&cameras, "camera", camera.as_deref(), false));
    form(&capture, 1, "Microphone", &picker(&mics, "mic", mic.as_deref(), false));
    group(column, Some("Capture"), &[capture.upcast()]);
    let speakers = [vec!["System default".to_owned()], devices::speakers()].concat();
    let siren = gtk::Grid::new();
    form(&siren, 0, "Speaker", &picker(&speakers, "speaker_name", Some(&current("speaker_name")), true));
    group(column, Some("Siren"), &[siren.upcast()]);
}

// ---- row builders -------------------------------------------------------------

/// A titled frame of rows.
pub(super) fn group(column: &gtk::Box, title: Option<&str>, rows: &[gtk::Widget]) {
    let frame = gtk::Frame::new(title);
    let stack = gtk::Box::new(gtk::Orientation::Vertical, 6);
    stack.set_border_width(8);
    for row in rows {
        stack.pack_start(row, false, false, 0);
    }
    frame.add(&stack);
    column.pack_start(&frame, false, false, 0);
}

/// Widgets left to right, the first one taking the slack.
fn row(items: &[gtk::Widget], spacing: i32) -> gtk::Widget {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 6 + spacing);
    for (i, item) in items.iter().enumerate() {
        line.pack_start(item, i == 0, i == 0, 0);
    }
    line.upcast()
}

fn form(grid: &gtk::Grid, at: i32, label: &str, field: &gtk::Widget) {
    grid.set_row_spacing(6);
    grid.set_column_spacing(12);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    grid.attach(&label, 0, at, 1, 1);
    field.set_hexpand(true);
    grid.attach(field, 1, at, 1, 1);
}

pub(super) fn switch(key: &'static str, text: &str) -> gtk::Widget {
    let check = gtk::CheckButton::with_label(text);
    check.set_active(app::config()[key] == true);
    check.connect_toggled(move |check| {
        let on = check.is_active();
        app::update_config(key, on.into());
        if key == "launch_at_login" {
            let _ = if on { loginitem::enable() } else { loginitem::disable() };
        }
    });
    check.upcast()
}

fn picker(items: &[String], key: &'static str, selected: Option<&str>, default_first: bool) -> gtk::Widget {
    let pop = gtk::ComboBoxText::new();
    for item in items {
        pop.append_text(item);
    }
    let at = selected.and_then(|s| items.iter().position(|item| item == s)).unwrap_or(0);
    pop.set_active(Some(at as u32));
    pop.connect_changed(move |pop| {
        let i = pop.active().unwrap_or(0);
        let value = if default_first && i == 0 { String::new() } else { pop.active_text().map(|t| t.to_string()).unwrap_or_default() };
        app::update_config(key, value.into());
    });
    pop.upcast()
}

fn short(path: &str) -> String {
    path.replace(&*crate::config::home().to_string_lossy(), "~")
}

// ---- the add-a-destination dialog ---------------------------------------------

fn small(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_line_wrap(true);
    label.set_xalign(0.0);
    label.style_context().add_class("dim-label");
    label
}

/// Returns a destination, or None if cancelled.
fn setup_telegram(parent: &gtk::Window) -> Option<Value> {
    let dialog = gtk::Dialog::with_buttons(
        Some("Add a Telegram destination"),
        Some(parent),
        gtk::DialogFlags::MODAL,
        &[("Cancel", gtk::ResponseType::Cancel), ("Save", gtk::ResponseType::Accept)],
    );
    dialog.set_icon(utils::app_icon().as_ref());
    let grid = gtk::Grid::new();
    grid.set_border_width(12);
    let entry = |placeholder: &str| {
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some(placeholder));
        entry.set_hexpand(true);
        entry
    };
    let (name, token, chat) = (entry("e.g. My phone"), entry("123456:ABC-DEF..."), entry("e.g. 987654321"));
    token.set_visibility(false);
    let detect = gtk::Button::with_label("Auto-detect");
    let hint = small("Click Auto-detect, then send your bot any message.");
    let test = gtk::Button::with_label("Send Test Message");
    let status = small("");
    let loud = gtk::Button::with_label("Make alerts ring on silent or Do Not Disturb…");
    loud.set_relief(gtk::ReliefStyle::None);
    let intro = gtk::Label::new(Some("Alerts (with photos and video) are sent here."));
    intro.set_xalign(0.0);
    grid.attach(&intro, 0, 0, 2, 1);
    form(&grid, 1, "Destination name", name.upcast_ref());
    form(&grid, 2, "Telegram bot token", token.upcast_ref());
    grid.attach(&gtk::LinkButton::with_label(TOKEN_HELP.1, TOKEN_HELP.0), 1, 3, 1, 1);
    form(&grid, 4, "Chat id", &row(&[chat.clone().upcast(), detect.clone().upcast()], 0));
    grid.attach(&hint, 1, 5, 1, 1);
    grid.attach(&row(&[status.clone().upcast(), test.clone().upcast()], 0), 1, 6, 1, 1);
    grid.attach(&loud, 1, 7, 1, 1);
    dialog.content_area().pack_start(&grid, true, true, 0);

    let value = |entry: &gtk::Entry| entry.text().trim().to_owned();
    let changed = {
        let (token, chat, detect, test) = (token.clone(), chat.clone(), detect.clone(), test.clone());
        move || {
            detect.set_sensitive(!value(&token).is_empty());
            test.set_sensitive(!value(&token).is_empty() && !value(&chat).is_empty());
        }
    };
    changed();
    for field in [&token, &chat] {
        let changed = changed.clone();
        field.connect_changed(move |_| changed());
    }
    let (found_in, again) = ((chat.clone(), hint.clone(), token.clone()), changed.clone());
    detect.connect_clicked(move |detect| {
        let (chat, hint, token) = found_in.clone();
        detect.set_sensitive(false);
        hint.set_text("Waiting… send your bot any message now.");
        // long poll off the main thread so the dialog stays responsive
        let (send, receive) = mpsc::channel();
        let key = value(&token);
        std::thread::spawn(move || {
            let _ = send.send(telegram::find_chat_id(&key, 30));
        });
        let again = again.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || match receive.try_recv() {
            Ok(found) => {
                match found {
                    Ok(id) => {
                        chat.set_text(&id);
                        hint.set_text("Found your chat.");
                    }
                    Err(why) => hint.set_text(&why),
                }
                again();
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(_) => glib::ControlFlow::Break,
        });
    });
    let (fields, said) = ((token.clone(), chat.clone()), status.clone());
    test.connect_clicked(move |_| {
        let result = telegram::send_test(&value(&fields.0), &value(&fields.1));
        said.set_text(&result.err().unwrap_or_else(|| "Sent. Check Telegram.".into()));
    });
    let owner = dialog.clone();
    loud.connect_clicked(move |_| {
        dialogs::alert_with("Make alerts loud on your phone", LOUD_HELP, &["Done"], None, Some(owner.upcast_ref()));
    });
    dialog.show_all();
    let mut result = None;
    while dialog.run() == gtk::ResponseType::Accept {
        let (n, t, c) = (value(&name), value(&token), value(&chat));
        if n.is_empty() || t.is_empty() || c.is_empty() {
            break;
        }
        match telegram::check(&t, &c) {
            Ok(()) => {
                result = Some(json!({"name": n, "token": t, "chat_id": c}));
                break;
            }
            Err(why) => {
                dialogs::alert_with("Setup failed", &format!("Telegram says: {why}"), &[], None, Some(dialog.upcast_ref()));
            }
        }
    }
    unsafe { dialog.destroy() };
    result
}
