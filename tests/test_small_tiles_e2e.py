"""Small app tiles (docs/app-guide.md, "Widget"; docs/recon.md, "Widget cards"): widget 0x40 with param
0x80 | (slot + 1) is laid out as a 166x166 tile, two to a row, by the stub's lookup in the card layout (stub 7,
card.rs `wp2_wgt_lookup`). From the host: two small app tiles share the top row, the small Apps tile sits under
them, and a tap starts exactly that app. From the Apps menu: "Add widget" on an app that declares WIDGET_SMALL
(hr-probe) adds the small tile. Needs target/app-wp2.bin (scripts/build-fw, stub version 7), target/loader.wp2app,
hello and hr-probe (scripts/build-app). Frames go to runs/small-tiles/."""
import json
import unittest
from unittest import skipUnless

from tests.e2e import harness
from tests.test_demo_e2e import save_png
from wp2apps import paths

OUT = paths.ROOT / "runs" / "small-tiles"
RUNNING = harness.STUB_RAM + 6
STUB = harness.STUB_RAM + 0xB66  # StubRam.stub (wp2-core layout.rs)
CARDS_STORE = 0x38007880  # wp2-core stock.rs
SMALL = 0x80
# the card's 2x2 grid of 166 px cells (docs/recon.md, "Grid")
LEFT, RIGHT, TOP, BOTTOM, SIDE = 61, 239, 61, 239, 166
ROW_Y = lambda row: 180 + 128 * row  # menu row centres, as harness.open_app
SHEET_Y = lambda i: 130 + 84 * i + 36  # sheet button centres: wp2-loader menu.rs sheet_y


def pixel(frame, x, y):
    w, _, rgb = frame
    return tuple(rgb[(y * w + x) * 3:(y * w + x) * 3 + 3])


def lit(frame, x, y):
    """A point inside a tile's rounded corner, above its text: the card grey there, black off a tile."""
    return max(pixel(frame, x + SIDE - 16, y + 12)) > 12


@skipUnless(harness.E2E, "set WP2_E2E=1 (needs the emulator, netsimd, gdb and llvm-nm)")
class SmallTilesTest(unittest.TestCase):
    def shot(self, emu, name):
        frame = emu.frame()
        save_png(OUT / f"{name}.png", *frame)
        return frame

    def tap(self, emu, x, y, pause=3):
        emu.send(f"t 1 {x} {y}", f"t 0 {x} {y}", pause=pause)

    def cards(self, emu):
        raw = emu.peek(CARDS_STORE, 0x60)
        return [raw[i:i + 12] for i in range(0, 0x60, 12) if raw[i] in (1, 2)]

    def test_small_tiles(self):
        OUT.mkdir(parents=True, exist_ok=True)
        with harness.installed(["loader", "hello", "hr-probe"], name="small-tiles") as (emu, port):
            harness.ready(emu)
            slots = {v["name"]: int(k) for k, v in json.loads(harness.cli_out(["list", "--hci-port", port]))["slots"].items()
                     if k != "L"}
            rows = sorted(slots, key=slots.get)  # the menu lists slots in order
            hello, hr = slots["hello"], slots["hr-probe"]
            self.assertEqual(emu.peek(STUB, 1)[0], 7, "the stub tells the loader its version")
            harness.ready(emu)  # the host's disconnect first: a second connection at once can fail
            harness.cli_out(["cards", "set", f"widgets:0x40/{hello + 1}s,0x40/{hr + 1}s,0x40/0s", "--hci-port", port])
            harness.ready(emu)

            harness.swipe(emu, 420, 233, 40, 233, steps=4)
            frame = self.shot(emu, "1-three-small")
            self.assertNotIn("wgt_reg not found", emu.uart())
            for name, x, y in (("hello", LEFT, TOP), ("hr-probe", RIGHT, TOP), ("Apps", LEFT, BOTTOM)):
                self.assertTrue(lit(frame, x, y), f"no small tile for {name} at {x}, {y}")
            self.assertFalse(lit(frame, RIGHT, BOTTOM), "the fourth cell is empty")
            self.assertEqual(pixel(frame, (LEFT + SIDE + RIGHT) // 2, TOP + 12), (0, 0, 0), "a gap between the two tiles")

            mark = len(emu.uart())
            self.tap(emu, RIGHT + SIDE // 2, TOP + SIDE // 2, pause=4)  # the right tile: hr-probe
            self.assertRegex(emu.uart()[mark:], r"view_stack: jump \d+ -> 240")
            self.assertEqual(emu.peek(RUNNING, 1)[0], hr, "the tap did not start hr-probe")
            self.shot(emu, "2-hr-probe-from-its-tile")
            emu.send("c 1", "c 0", pause=3)  # BUTTON: the app writes its record and returns to the menu
            self.assertEqual(emu.peek(RUNNING, 1)[0], 0xFF, "hr-probe did not exit")
            rec = harness.read_file(emu, f"/NAND:/wp2apps/{hr}.wgt".encode(), 64)
            self.assertEqual((rec or b"")[:24].rstrip(b"\0"), b"Pulse")
            emu.send("c 1", "c 0", pause=3)  # menu -> face, which lays the cards out again
            harness.ready(emu)
            harness.swipe(emu, 420, 233, 40, 233, steps=4)
            self.shot(emu, "3-record-on-the-small-tile")
            emu.send("c 1", "c 0", pause=3)

            # from the menu: hr-probe declares WIDGET_SMALL, so "Add widget" adds the small tile after the wide Apps one
            harness.ready(emu)
            harness.cli_out(["cards", "set", "widgets:0x40", "--hci-port", port])
            harness.open_menu(emu)
            emu.send(f"t 1 200 {ROW_Y(rows.index('hr-probe'))}", pause=1.5)
            emu.send(f"t 0 200 {ROW_Y(rows.index('hr-probe'))}", pause=2)
            self.shot(emu, "4-sheet-hr-probe")  # Add widget, Delete, Cancel
            self.tap(emu, 233, SHEET_Y(0))
            cards = self.cards(emu)
            self.assertEqual(cards[0][:7], bytes([1, 0, 2, 0x40, 0, 0x40, hr + 1 | SMALL]), "the small tile, after Apps")
            emu.send("c 1", "c 0", pause=3)  # crown: the menu goes, the face lays the cards out
            harness.ready(emu)
            harness.swipe(emu, 420, 233, 40, 233, steps=4)
            frame = self.shot(emu, "5-apps-and-small")
            self.assertTrue(lit(frame, LEFT, BOTTOM), "hr-probe's small tile under the wide Apps tile")
            self.assertFalse(lit(frame, RIGHT, BOTTOM), "a small tile is half the row")
            self.assertNotIn("FATAL", emu.uart().upper())


if __name__ == "__main__":
    unittest.main()
