import subprocess
import rumps
from pyobjc_framework_Cocoa import NSAppleScript, NSUserDefaults

def show_alert(title: str, message: str, ok: bool = True, cancel: bool = False) -> bool:
    """Show simple alert dialog."""
    response = rumps.alert(title=title, message=message, ok=ok, cancel=cancel)
    return response == "OK" if ok else (response == 1)

def show_input_dialog(title: str, message: str, default: str = "") -> str:
    """Show input dialog."""
    return rumps.Window(message=message, default_text=default, ok="OK", cancel="Cancel", dimensions=(300, 25)).run().text or None

def lock_screen() -> None:
    """Lock screen via AppleScript key combo (Ctrl+Cmd+Q)."""
    applescript = '''
    tell application "System Events"
        key code 12 using {command down, control down}
    end tell
    '''
    NSAppleScript.alloc().initWithSource_(applescript).executeAndReturnError_(None)

def add_to_login_items(app_path: str) -> None:
    """Add app to Login Items using LSSharedFileList (PyObjC)."""
    from pyobjc_framework_CoreServices import LSSharedFileList, kLSSharedFileListSessionLoginItems
    login_items = LSSharedFileListGetSharedFileList(kLSSharedFileListSessionLoginItems, None)
    if login_items:
        LSSharedFileListInsertItemURL(login_items, kLSSharedFileListItemLast, None, None, app_path.encode(), None, None)

def remove_from_login_items(app_path: str) -> None:
    """Remove app from Login Items."""
    from pyobjc_framework_CoreServices import LSSharedFileList, kLSSharedFileListSessionLoginItems
    login_items = LSSharedFileListGetSharedFileList(kLSSharedFileListSessionLoginItems, None)
    if login_items:
        # Find and remove item (simplified; match by URL)
        items = LSSharedFileListCopySnapshot(login_items, None).get()
        for i, item in enumerate(items or []):
            url = LSSharedFileListItemCopyResolvedURL(item, 0, None).takeRetainedValue()
            if url and app_path.encode() in str(url):
                LSSharedFileListItemRemove(login_items, i)
                break

def get_app_path() -> str:
    """Get bundled app path or script dir for dev."""
    return rumps.app().path or __file__
