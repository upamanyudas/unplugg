import sys

if sys.platform == "darwin":
    from unplugg.app import UnpluggApp
else:
    from unplugg.pc.app import UnpluggApp

if __name__ == "__main__":
    UnpluggApp().run()
