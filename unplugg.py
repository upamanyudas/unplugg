import rumps
from config import load_config, save_config
from monitor import Monitor
from alarm import AlarmHandler
from utils import show_alert, show_input_dialog, add_to_login_items, remove_from_login_items, get_app_path

class UnpluggApp(rumps.App):
    def __init__(self):
        super(UnpluggApp, self).__init__("🔌", title="unplugg")  # Icon placeholder
        self.config = load_config()
        self.monitor = Monitor(self._on_unsafe, self.config)
        self.alarm_handler = AlarmHandler(self.config)
        self.app_path = get_app_path()
        self._update_menu()
        if self.config["launch_at_login"]:
            add_to_login_items(self.app_path)

    @rumps.clicked("Activate")
    def activate(self, _):
        if not self.config["activated"]:
            if self.config["monitor_power"] and not self.monitor._get_power_state():
                show_alert("Activation Failed", "Your MacBook should be plugged to power before you can activate the alarm. Alternatively, uncheck 'On Power Unplugged' option.")
                return
            self.config["activated"] = True
            self.monitor.start()
            self.menu["Activate"].set_title("Deactivate")
        else:
            self.config["activated"] = False
            self.monitor.stop()
            self.menu["Activate"].set_title("Activate")
        save_config(self.config)
        self._update_menu()

    def _update_menu(self):
        # Alarm On section
        power_menu = rumps.MenuItem("Power Unplugged", self.config["monitor_power"])
        power_menu.set_callback(self._toggle_monitor_power)
        lid_menu = rumps.MenuItem("Screen Closed", self.config["monitor_lid"])
        lid_menu.set_callback(self._toggle_monitor_lid)
        alarm_on = rumps.MenuItem("Alarm On", [power_menu, lid_menu])

        # Alarm Actions
        sound_menu = rumps.MenuItem("Play Siren Sound", self.config["play_sound"])
        sound_menu.set_callback(self._toggle_sound)
        media_menu = rumps.MenuItem("Send Photo & Video to Telegram", self.config["capture_media"])
        media_menu.set_callback(self._toggle_media)

        # Notifications submenu
        notif_sub = rumps.MenuItem("Notifications")
        for dest in self.config["telegram_destinations"]:
            dest_menu = rumps.MenuItem(dest["name"])
            # Toggle? Or edit
            notif_sub.add_item(dest_menu)
        setup_menu = rumps.MenuItem("Setup a new destination")
        setup_menu.set_callback(self._setup_telegram)
        notif_sub.add_item(setup_menu)

        actions = rumps.MenuItem("Alarm Actions", [sound_menu, media_menu, notif_sub])

        # When Active
        overlay_menu = rumps.MenuItem("Show Overlay Warning", self.config["show_overlay"])
        overlay_menu.set_callback(self._toggle_overlay)
        lock_menu = rumps.MenuItem("Lock Device", self.config["lock_device"])
        lock_menu.set_callback(self._toggle_lock)
        active = rumps.MenuItem("When Active:", [overlay_menu, lock_menu])

        # Launch
        launch_menu = rumps.MenuItem("Launch at login", self.config["launch_at_login"])
        launch_menu.set_callback(self._toggle_launch)

        # About/Help/Quit
        about = rumps.MenuItem("About...")
        about.set_callback(self._show_about)
        help_ = rumps.MenuItem("Help...")
        help_.set_callback(self._show_help)
        quit_ = rumps.MenuItem("Quit")

        self.menu.clear()
        self.menu.update([
            rumps.separator,
            self.menu["Activate"],
            rumps.separator,
            alarm_on,
            rumps.separator,
            actions,
            rumps.separator,
            active,
            rumps.separator,
            launch_menu,
            rumps.separator,
            about, help_, quit_
        ])

    def _toggle_monitor_power(self, sender):
        self.config["monitor_power"] = not sender.state
        sender.set_state(self.config["monitor_power"])
        save_config(self.config)

    def _toggle_monitor_lid(self, sender):
        self.config["monitor_lid"] = not sender.state
        sender.set_state(self.config["monitor_lid"])
        save_config(self.config)

    def _toggle_sound(self, sender):
        self.config["play_sound"] = not sender.state
        sender.set_state(self.config["play_sound"])
        save_config(self.config)

    def _toggle_media(self, sender):
        self.config["capture_media"] = not sender.state
        sender.set_state(self.config["capture_media"])
        save_config(self.config)

    def _setup_telegram(self, _):
        name = show_input_dialog("Setup Telegram", "Destination name:")
        if not name:
            return
        token_help = "Get bot token: https://core.telegram.org/bots#6-botfather"
        token = show_input_dialog("Telegram Bot Token", f"Bot Token:\n{token_help}")
        if not token:
            return
        chat_help = "Get chat ID: https://api.telegram.org/bot<TOKEN>/getUpdates"
        chat_id = show_input_dialog("Chat ID", f"Chat ID:\n{chat_help}")
        if not chat_id:
            return
        self.config["telegram_destinations"].append({"name": name, "token": token, "chat_id": chat_id})
        save_config(self.config)
        self._update_menu()

    def _toggle_overlay(self, sender):
        self.config["show_overlay"] = not sender.state
        sender.set_state(self.config["show_overlay"])
        save_config(self.config)

    def _toggle_lock(self, sender):
        self.config["lock_device"] = not sender.state
        sender.set_state(self.config["lock_device"])
        save_config(self.config)

    def _toggle_launch(self, sender):
        self.config["launch_at_login"] = not sender.state
        sender.set_state(self.config["launch_at_login"])
        if self.config["launch_at_login"]:
            add_to_login_items(self.app_path)
        else:
            remove_from_login_items(self.app_path)
        save_config(self.config)

    def _show_about(self, _):
        show_alert("About unplugg", "unplugg v1.0\nSafeguard your Mac in public.\n© 2025 Example Inc.\n[Logo placeholder]")

    def _show_help(self, _):
        help_text = """
        [Placeholder for help images and text]
        1. Activate monitoring.
        2. Setup Telegram for alerts.
        3. Customize actions.
        [Add screenshots later]
        """
        show_alert("Help - unplugg", help_text, ok=True)

    def _on_unsafe(self, event: str):
        self.alarm_handler.trigger(event)

    @rumps.clicked("Quit")
    def quit(self, _):
        self.monitor.stop()
        if not self.config["launch_at_login"]:
            remove_from_login_items(self.app_path)
        rumps.quit_application()
