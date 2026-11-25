use gtk::glib;
use gtk::prelude::*;

use super::utils;

/// Shows an alert, with an optional widget under the text; returns the index of
/// the button clicked (0 = first). Closing it counts as the last button.
pub fn alert_with(title: &str, text: &str, buttons: &[&str], accessory: Option<&gtk::Widget>, parent: Option<&gtk::Window>) -> usize {
    let buttons = if buttons.is_empty() { &["OK"][..] } else { buttons };
    let dialog = gtk::Dialog::new();
    dialog.set_title("unplugg");
    dialog.set_icon(utils::app_icon().as_ref());
    dialog.set_transient_for(parent);
    dialog.set_modal(true);
    dialog.set_resizable(false);
    // a tray app is never in front by itself
    dialog.set_keep_above(true);
    for (i, label) in buttons.iter().enumerate() {
        dialog.add_button(label, gtk::ResponseType::Other(i as u16));
    }
    dialog.set_default_response(gtk::ResponseType::Other(0));

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row.set_border_width(12);
    if let Some(icon) = utils::app_icon().and_then(|icon| icon.scale_simple(64, 64, gtk::gdk_pixbuf::InterpType::Bilinear)) {
        let image = gtk::Image::from_pixbuf(Some(&icon));
        image.set_valign(gtk::Align::Start);
        row.pack_start(&image, false, false, 0);
    }
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let heading = gtk::Label::new(None);
    heading.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(title)));
    let body = gtk::Label::new(Some(text));
    body.set_line_wrap(true);
    body.set_max_width_chars(60);
    for label in [&heading, &body] {
        label.set_xalign(0.0);
        label.set_selectable(false);
        column.pack_start(label, false, false, 0);
    }
    if let Some(accessory) = accessory {
        column.pack_start(accessory, true, true, 0);
    }
    row.pack_start(&column, true, true, 0);
    dialog.content_area().pack_start(&row, true, true, 0);
    dialog.show_all();
    let response = dialog.run();
    unsafe { dialog.destroy() };
    match response {
        gtk::ResponseType::Other(i) => i as usize,
        _ => buttons.len() - 1,
    }
}

pub fn alert(title: &str, text: &str, buttons: &[&str]) -> usize {
    alert_with(title, text, buttons, None, None)
}

pub fn show_about() {
    alert(
        &format!("unplugg {}", env!("CARGO_PKG_VERSION")),
        "Your laptop, guarded while you step away.\n\nCopyright © 2025 Upa Das. MIT License.",
        &[],
    );
}

pub fn show_help() {
    alert(
        "How unplugg works",
        "1. Plug in and add a Telegram destination.\n\
         2. Choose what counts as unsafe: power unplugged, lid closed, or moved.\n\
         3. Click Activate in unplugg's tray menu before you step away.\n\n\
         When the laptop is disturbed: it sirens, captures photo + video, sends a location link, and alerts Telegram, then optionally locks.",
        &["Done"],
    );
}
