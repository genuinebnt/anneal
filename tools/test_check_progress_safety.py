#!/usr/bin/env python3
"""Self-test of tools/check-progress-safety.py's course stage check: `python3 tools/test_check_progress_safety.py`.

Runs the check on dictionaries, then end to end in a throwaway git repo (a copy of the script, one course) whose base commit has
stages that the working tree renames, splits, merges or drops.
"""
import importlib.util
import pathlib
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parent / "check-progress-safety.py"
spec = importlib.util.spec_from_file_location("check_progress_safety", SCRIPT)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


def stages(*pairs):
    """{"c": (ids, former ids)} from (id, renamed_from) pairs."""
    return check.stages_of((f"courses/c/modules/01-a/stages/{i:02}/stage.toml", f"id = \"{sid}\"\nrenamed_from = {former}\n")
                           for i, (sid, former) in enumerate(pairs))


class StageProblems(unittest.TestCase):
    def test_unchanged_stages_pass(self):
        base = stages(("1a-01", []), ("1a-02", []))
        self.assertEqual(check.stage_problems(base, base), [])

    def test_a_stage_that_disappears_without_renamed_from_fails(self):
        problems = check.stage_problems(stages(("1a-01", []), ("1a-02", [])), stages(("1a-01", [])))
        self.assertEqual(len(problems), 1)
        self.assertIn("stage 1a-02 disappeared", problems[0])
        self.assertIn('renamed_from = ["1a-02"]', problems[0])

    def test_a_rename_a_split_and_a_merge_pass(self):
        base = stages(("1a-01", []), ("1a-02", []), ("1a-03", []), ("1a-04", []))
        now = stages(("2a-01", ["1a-01"]), ("2a-02", ["1a-02"]), ("2a-03", ["1a-02"]), ("2a-04", ["1a-03", "1a-04"]))
        self.assertEqual(check.stage_problems(base, now), [])

    def test_dropping_an_old_id_from_renamed_from_fails(self):
        base = stages(("2a-01", ["1a-01"]))
        problems = check.stage_problems(base, stages(("3a-01", ["2a-01"])))
        self.assertEqual(len(problems), 1)
        self.assertIn("1a-01 is no longer listed", problems[0])
        self.assertEqual(check.stage_problems(base, stages(("3a-01", [["2a-01", "1a-01"]]))), [], "renamed twice: one entry, newest first")

    def test_courses_are_compared_apart(self):
        base = {"a": ({"1a-01"}, set()), "b": ({"1a-01"}, set())}
        problems = check.stage_problems(base, {"a": ({"1a-01"}, set())})
        self.assertEqual(len(problems), 1)
        self.assertIn("course b", problems[0])


class EndToEnd(unittest.TestCase):
    def setUp(self):
        self.repo = pathlib.Path(tempfile.mkdtemp(prefix="anneal-safety-"))
        (self.repo / "tools").mkdir()
        shutil.copy(SCRIPT, self.repo / "tools" / SCRIPT.name)
        (self.repo / "content/tracks").mkdir(parents=True)
        self.write("01", 'id = "1a-01"')
        self.write("02", 'id = "1a-02"')
        self.git("init", "-q")
        self.git("add", "-A")
        self.git("-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-q", "-m", "base")

    def tearDown(self):
        shutil.rmtree(self.repo)

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.repo, check=True, capture_output=True)

    def write(self, dir, toml):
        stage = self.repo / "courses/c/modules/01-a/stages" / dir
        stage.mkdir(parents=True, exist_ok=True)
        (stage / "stage.toml").write_text(toml + '\ntitle = "S"\n')

    def run_check(self):
        r = subprocess.run([sys.executable, "tools/check-progress-safety.py", "HEAD"], cwd=self.repo, capture_output=True, text=True)
        return r.returncode, r.stdout + r.stderr

    def test_the_script_fails_a_dropped_stage_and_passes_once_it_is_claimed(self):
        self.assertEqual(self.run_check()[0], 0)
        self.write("02", 'id = "1b-01"')
        code, out = self.run_check()
        self.assertEqual(code, 1, out)
        self.assertIn("stage 1a-02 disappeared", out)
        self.write("02", 'id = "1b-01"\nrenamed_from = ["1a-02"]')
        code, out = self.run_check()
        self.assertEqual(code, 0, out)
        self.assertIn("2 course stages", out)


if __name__ == "__main__":
    unittest.main()
