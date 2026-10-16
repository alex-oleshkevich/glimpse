import dataclasses
import json
import pathlib
import tempfile
import unittest

from glimpse_sdk import BaseApplet, Context, Indicator, Session, State, W


@dataclasses.dataclass(frozen=True)
class Counter(State):
    count: int = 0
    wide: bool = False


class Applet(BaseApplet[Counter]):
    state = Counter()
    persist = ("count",)

    def build(self, ctx: Context) -> list[W]:
        return [Indicator(text=str(self.state.count))]


def mounted(state_dir: str = "") -> tuple[Applet, list[bool]]:
    woken: list[bool] = []
    applet = Applet()
    session = Session(applet, lambda line: None, on_wake=lambda: woken.append(True))
    session.ctx.state_dir = state_dir
    applet.mount(session.ctx)
    woken.clear()
    return applet, woken


class BaseAppletTest(unittest.TestCase):
    def test_set_state_replaces_the_state_and_wakes(self) -> None:
        applet, woken = mounted()
        applet.set_state(wide=True)
        self.assertTrue(applet.state.wide)
        self.assertEqual(woken, [True])

    def test_an_unchanged_state_does_not_wake(self) -> None:
        applet, woken = mounted()
        applet.set_state(count=0)
        self.assertEqual(woken, [])

    def test_an_unknown_field_is_a_type_error(self) -> None:
        applet, _ = mounted()

        with self.assertRaises(TypeError):
            applet.set_state(wdie=True)

    def test_class_level_default_state_is_not_shared_after_a_change(self) -> None:
        applet, _ = mounted()
        applet.set_state(count=5)
        self.assertEqual(Applet.state.count, 0)

    def test_persisted_fields_are_saved_and_loaded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            applet, _ = mounted(directory)
            applet.set_state(count=3, wide=True)
            saved = json.loads((pathlib.Path(directory) / "state.json").read_text())
            self.assertEqual(saved, {"count": 3})
            reloaded, _ = mounted(directory)
            self.assertEqual(reloaded.state.count, 3)
            self.assertFalse(reloaded.state.wide)

    def test_a_non_persisted_change_writes_nothing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            applet, _ = mounted(directory)
            applet.set_state(wide=True)
            self.assertFalse((pathlib.Path(directory) / "state.json").exists())

    def test_a_corrupt_state_file_is_ignored(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            (pathlib.Path(directory) / "state.json").write_text("{nope")
            applet, _ = mounted(directory)
            self.assertEqual(applet.state.count, 0)


if __name__ == "__main__":
    unittest.main()
