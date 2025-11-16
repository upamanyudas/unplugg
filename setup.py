from setuptools import setup

APP = ['main.py']
DATA_FILES = ['assets']
OPTIONS = {
    'argv_emulation': True,
    'packages': ['rumps', 'opencv', 'requests', 'playsound', 'pyobjc'],
    'plist': {
        'CFBundleName': 'unplugg',
        'CFBundleDisplayName': 'unplugg',
        'CFBundleIdentifier': 'com.example.unplugg',
        'CFBundleVersion': '1.0.0',
        'CFBundleShortVersionString': '1.0.0',
        'NSPrincipalClass': 'NSApplication',
        'LSMinimumSystemVersion': '10.15',  # Catalina
        'LSUIElement': True,  # Menu bar only
    },
    'iconfile': 'assets/logo.icns',  # Convert logo.png to .icns
}

setup(
    app=APP,
    data_files=DATA_FILES,
    options={'py2app': OPTIONS},
    setup_requires=['py2app'],
)
