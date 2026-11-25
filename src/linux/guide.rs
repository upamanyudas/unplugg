//! Setup guide: opens on first launch, and from the menu's Setup Guide….
//! A wizard with Settings' own rows: how it works, Telegram, launch at login.
//! It's optional: closing it early still counts as seen.

use std::cell::RefCell;

use gtk::prelude::*;

use super::{app, ui, utils};

const HOW: [(&str, &str); 4] = [
    ("Plug in", "unplugg watches the power cable and the lid."),
    ("Activate", "Click unplugg in the taskbar's tray before you step away."),
    ("If it's unplugged or moved", "A siren sounds. Photos, video and location go to your phone."),
    ("Only you can stop it", "Turning it off asks for your password."),
];
const TRAY: &str = "On GNOME, tray icons need the AppIndicator extension (Ubuntu has it already).";

thread_local! {
    static GUIDE: RefCell<Option<gtk::Assistant>> = const { RefCell::new(None) };
}

fn label(markup: &str) -> gtk::Widget {
    let label = gtk::Label::new(None);
    label.set_markup(markup);
    label.set_line_wrap(true);
    label.set_xalign(0.0);
    label.upcast()
}

pub fn open() {
    if let Some(guide) = GUIDE.with(|g| g.borrow().clone()) {
        guide.present();
        return;
    }
    let guide = gtk::Assistant::new();
    guide.set_title("unplugg Setup Guide");
    guide.set_icon(utils::app_icon().as_ref());
    let window: gtk::Window = guide.clone().upcast();
    let steps: [(&str, &str, Box<dyn Fn(&gtk::Box)>); 3] = [
        (
            "Welcome to unplugg",
            "Your laptop, guarded while you step away.",
            Box::new(|column| {
                let rows: Vec<gtk::Widget> = HOW.iter().map(|(title, line)| label(&format!("<b>{}</b>\n{}", gtk::glib::markup_escape_text(title), gtk::glib::markup_escape_text(line)))).collect();
                ui::group(column, None, &rows);
            }),
        ),
        (
            "Get alerts on your phone",
            "When your laptop is disturbed, a Telegram bot sends you photos, video and where it is. Click + to add one.",
            Box::new(move |column| ui::telegram_group(&window, column)),
        ),
        (
            "You're all set",
            "Before you step away, click unplugg in the tray and choose Activate.",
            Box::new(|column| {
                ui::group(column, None, &[ui::switch("launch_at_login", "Launch at login")]);
                column.pack_start(&label(TRAY), false, false, 0);
            }),
        ),
    ];
    let last = steps.len() - 1;
    for (i, (title, subtitle, body)) in steps.iter().enumerate() {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
        column.set_border_width(12);
        // the title and subtitle, the logo on the right, as the wizard showed them
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.pack_start(&label(&format!("<big><b>{}</b></big>\n{}", gtk::glib::markup_escape_text(title), gtk::glib::markup_escape_text(subtitle))), true, true, 0);
        if let Some(icon) = utils::app_icon().and_then(|icon| icon.scale_simple(64, 64, gtk::gdk_pixbuf::InterpType::Bilinear)) {
            header.pack_end(&gtk::Image::from_pixbuf(Some(&icon)), false, false, 0);
        }
        column.pack_start(&header, false, false, 0);
        body(&column);
        guide.append_page(&column);
        guide.set_page_title(&column, title);
        guide.set_page_complete(&column, true);
        let kind = if i == last { gtk::AssistantPageType::Summary } else { gtk::AssistantPageType::Content };
        guide.set_page_type(&column, kind);
    }
    // Done, Cancel or the window's close: all count as seen
    guide.connect_close(|guide| finish(guide));
    guide.connect_cancel(|guide| finish(guide));
    guide.connect_delete_event(|guide, _| {
        finish(guide);
        gtk::glib::Propagation::Stop
    });
    guide.show_all();
    guide.present();
    GUIDE.with(|g| *g.borrow_mut() = Some(guide));
}

fn finish(guide: &gtk::Assistant) {
    app::update_config("setup_done", true.into());
    GUIDE.with(|g| g.borrow_mut().take());
    unsafe { guide.destroy() };
}
