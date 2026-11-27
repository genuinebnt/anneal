"""Hand-written lessons survive a run of the converter, on a made-up repository.

    python3 -m unittest discover -s tools/cookbook -p 'test_*.py'
"""
import contextlib, io, json, pathlib, sys, tempfile, unittest

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import convert

GUIDE = '<section><h2>{title}</h2><p>Some words about it.</p></section>'
LESSON = 'title = "written by hand"\n'


class Lessons(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = pathlib.Path(folder.name)
        self.addCleanup(setattr, convert, "ROOT", convert.ROOT)
        convert.ROOT = self.root
        self.write("db.html", GUIDE.format(title="Joins"))
        self.write("co.html", GUIDE.format(title="Parsing"))
        self.write("cookbook/examples/a/queue.rs", "//! A queue.\n//!\n//! It holds things.\n\nfn main() {}\n")
        self.outline([["db", "Joins"]])
        self.out = self.root / "content/cookbook"

    def write(self, path, text):
        path = self.root / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def outline(self, guides):
        chapters = [{"title": "Chapter", "sections": guides + [["co", "Parsing"]], "recipes": ["examples/a/queue.rs"]}]
        self.write("tools/cookbook/outline.json", json.dumps([{"subject": "data", "module": "Module", "order": 1, "chapters": chapters}]))

    def run_converter(self):
        with contextlib.redirect_stdout(io.StringIO()):
            convert.main(str(self.root / "db.html"), str(self.root / "co.html"))

    def files(self):
        return {str(f.relative_to(self.out)): f.read_text() for f in self.out.rglob("*.toml")}

    def test_a_run_leaves_lessons_and_every_page_address_unchanged(self):
        self.run_converter()
        before = self.files()
        self.assertEqual(sorted(before), ["data/joins.toml", "data/parsing.toml", "data/queue.toml"])
        self.write("content/cookbook/lessons/data/queue.toml", LESSON)
        self.write("content/cookbook/lessons/data/joins.toml", LESSON)
        self.run_converter()
        after = self.files()
        self.assertEqual(after.pop("lessons/data/queue.toml"), LESSON)
        self.assertEqual(after.pop("lessons/data/joins.toml"), LESSON)
        # no suffixed second copy of a lesson's page, and the generated pages are as they were
        self.assertEqual(after, before)

    def test_a_run_that_would_strand_a_lesson_changes_nothing(self):
        self.run_converter()
        self.write("content/cookbook/lessons/data/joins.toml", LESSON)
        before = self.files()
        self.outline([])
        with self.assertRaises(SystemExit) as stopped:
            self.run_converter()
        self.assertIn("content/cookbook/lessons/data/joins.toml", str(stopped.exception))
        self.assertEqual(self.files(), before)

    def test_a_lesson_in_the_wrong_place_is_not_deleted(self):
        for place in ["lessons/queue.toml", "lessons/systems/queue.toml"]:
            self.write(f"content/cookbook/{place}", LESSON)
            with self.assertRaises(SystemExit):
                self.run_converter()
            self.assertEqual(self.files(), {place: LESSON})
            (self.out / place).unlink()

    def test_drawings_that_ran_over_are_repaired_on_every_run(self):
        old, new = convert.DRAWING_FIXES[0]
        # the guide names its classes without the prefix the converter adds
        figure = f'<section><h2>Joins</h2><figure class="fig"><svg viewBox="0 0 820 190">{old.replace("cb-g-", "")}</svg></figure></section>'
        self.write("db.html", figure)
        self.run_converter()
        made = self.files()["data/joins.toml"]
        self.assertNotIn('x=\\"520\\"', made)
        self.assertIn(">child i holds keys in [key_i, key_{i+1})</text>", made)
        self.assertEqual(convert.fix_drawings(new), new)

    def test_a_cheat_sheet_splits_at_the_arrow_between_its_two_columns(self):
        split = convert.code_pages.layout.split_at_arrow
        for entry, halves in [
            ("infallible widening (u8 -> u32)      -> From / Into", ("infallible widening (u8 -> u32)", "From / Into")),
            ("path <-> file URL      -> to_file_path", ("path <-> file URL", "to_file_path")),
            ("OS string -> &str                  -> to_str()", ("OS string -> &str", "to_str()")),
            ("fallible narrowing -> TryFrom / try_into", ("fallible narrowing", "TryFrom / try_into")),
            ("exit code      -> main() -> ExitCode", ("exit code", "main() -> ExitCode")),
            ("only in brackets (a -> b)", None),
        ]:
            self.assertEqual(split(entry), halves, entry)


if __name__ == "__main__":
    unittest.main()
