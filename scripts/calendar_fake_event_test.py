import contextlib
import importlib.util
import io
import os
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory


SCRIPT_PATH = Path(__file__).with_name("calendar-fake-event.py")


def load_script():
    spec = importlib.util.spec_from_file_location("calendar_fake_event", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class CalendarFakeEventTest(unittest.TestCase):
    def test_default_config_path_uses_xdg_config_home(self):
        module = load_script()

        with TemporaryDirectory() as home, TemporaryDirectory() as xdg, TemporaryDirectory() as cwd:
            old_env = os.environ.copy()
            old_cwd = Path.cwd()
            try:
                os.environ.pop("GLIMPSE_CONFIG", None)
                os.environ["HOME"] = home
                os.environ["XDG_CONFIG_HOME"] = xdg
                os.chdir(cwd)

                self.assertEqual(
                    module.default_config_path(),
                    Path(xdg) / "glimpse" / "config.toml",
                )
            finally:
                os.chdir(old_cwd)
                os.environ.clear()
                os.environ.update(old_env)

    def test_config_hint_includes_fast_poll_interval(self):
        module = load_script()

        with TemporaryDirectory() as temp_dir:
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                module.print_config_hint(
                    Path(temp_dir) / "missing.toml",
                    Path(temp_dir) / "events",
                )

            self.assertIn("poll_interval = 60", output.getvalue())


if __name__ == "__main__":
    unittest.main()
