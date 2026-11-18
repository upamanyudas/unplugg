"""Qt dialogs: alerts, the Telegram setup dialog, About, Help."""
import html
import threading

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QDialog, QDialogButtonBox, QFormLayout, QHBoxLayout, QLabel, QLineEdit, QMessageBox, QPushButton,
)

from .. import __version__, telegram
from ..telegram import LOUD_HELP, TOKEN_HELP
from .utils import app_icon, call_after


def alert(title, text, *buttons, accessory=None, parent=None):
    """Show an alert, with an optional widget under the text; returns the index of
    the button clicked (0 = first)."""
    box = QMessageBox(parent)
    box.setWindowTitle("unplugg")
    box.setWindowIcon(app_icon())
    box.setIconPixmap(app_icon().pixmap(64, 64))
    box.setText(f"<b>{html.escape(title)}</b>")
    box.setInformativeText(text)
    added = [box.addButton(b, QMessageBox.ButtonRole.ActionRole) for b in buttons or ["OK"]]
    box.setDefaultButton(added[0])
    box.setEscapeButton(added[-1])
    if accessory:
        grid = box.layout()
        grid.addWidget(accessory, grid.rowCount(), 0, 1, grid.columnCount())
    box.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint)  # a tray app is never in front by itself
    box.exec()
    return added.index(box.clickedButton()) if box.clickedButton() in added else len(added) - 1


def setup_telegram(parent=None):
    """The add-a-destination dialog. Returns a destination dict or None."""
    dialog = QDialog(parent)
    dialog.setWindowTitle("Add a Telegram destination")
    dialog.setWindowIcon(app_icon())
    form = QFormLayout(dialog)
    form.addRow(QLabel("Alerts (with photos and video) are sent here."))
    name, token, chat = QLineEdit(), QLineEdit(), QLineEdit()
    name.setPlaceholderText("e.g. My phone")
    token.setPlaceholderText("123456:ABC-DEF...")
    token.setEchoMode(QLineEdit.EchoMode.Password)
    chat.setPlaceholderText("e.g. 987654321")
    form.addRow("Destination name", name)
    form.addRow("Telegram bot token", token)
    form.addRow("", _link(*TOKEN_HELP))
    detect = QPushButton("Auto-detect")
    row = QHBoxLayout()
    row.addWidget(chat)
    row.addWidget(detect)
    form.addRow("Chat id", row)
    hint = _small("Click Auto-detect, then send your bot any message.")
    form.addRow("", hint)
    test, status = QPushButton("Send Test Message"), _small("")
    row = QHBoxLayout()
    row.addWidget(test)
    row.addWidget(status, 1)
    form.addRow("", row)
    loud = QPushButton("Make alerts ring on silent or Do Not Disturb…")
    loud.setFlat(True)
    loud.clicked.connect(lambda: alert("Make alerts loud on your phone", LOUD_HELP, "Done", parent=dialog))
    form.addRow("", loud)
    buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Save | QDialogButtonBox.StandardButton.Cancel)
    buttons.accepted.connect(dialog.accept)
    buttons.rejected.connect(dialog.reject)
    form.addRow(buttons)

    def value(field):
        return field.text().strip()

    def changed():
        detect.setEnabled(bool(value(token)))
        test.setEnabled(bool(value(token) and value(chat)))

    def detected(result):
        found, err = result
        if found:
            chat.setText(found)
        hint.setText("Found your chat." if found else err)
        changed()

    def find():
        detect.setEnabled(False)
        hint.setText("Waiting… send your bot any message now.")
        t = value(token)  # long poll off the main thread so the dialog stays responsive
        threading.Thread(target=lambda: call_after(detected, telegram.find_chat_id(t)), daemon=True).start()

    detect.clicked.connect(find)
    test.clicked.connect(lambda: status.setText(telegram.send_test(value(token), value(chat))
                                                or "Sent. Check Telegram."))
    token.textChanged.connect(changed)
    chat.textChanged.connect(changed)
    changed()

    while dialog.exec() == QDialog.DialogCode.Accepted:
        n, t, c = value(name), value(token), value(chat)
        if not (n and t and c):
            return None
        err = telegram.check(t, c)
        if not err:
            return {"name": n, "token": t, "chat_id": c}
        alert("Setup failed", f"Telegram says: {err}", parent=dialog)
    return None


def _small(text):
    label = QLabel(text)
    label.setWordWrap(True)
    label.setStyleSheet("color: palette(placeholder-text)")
    return label


def _link(text, url):
    label = QLabel(f'<a href="{url}">{text}</a>')
    label.setOpenExternalLinks(True)
    return label


def show_about():
    alert(f"unplugg {__version__}", "Your laptop, guarded while you step away.\n\n"
          "Copyright © 2025 Upa Das. MIT License.")


def show_help():
    alert("How unplugg works",
          "1. Plug in and add a Telegram destination.\n"
          "2. Choose what counts as unsafe: power unplugged, lid closed, or moved.\n"
          "3. Click Activate in unplugg's tray menu before you step away.\n\n"
          "When the laptop is disturbed: it sirens, captures photo + video, sends a "
          "location link, and alerts Telegram, then optionally locks.",
          "Done")
