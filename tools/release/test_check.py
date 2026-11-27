#!/usr/bin/env python3
"""Self-test for tools/release/check, against a stand-in for gh.

    python3 -I -B tools/release/test_check.py

Each test writes a small release into a temporary directory: its tag, its
files and their attestations, described in a JSON file. A stand-in `gh` on
PATH answers the four calls the script makes from that file. Its
`attestation verify` applies each flag the way gh does: a flag that is left
out matches any attestation. So a test whose only attestation comes from a
dry run on a branch, from another workflow, from another commit, or from a
self-hosted runner fails only while the script passes the flag that rules
that attestation out.
"""

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "check"
REPO = "vtium/vtium"
TAG = "v0.1.0-alpha.12"
VERSION = "0.1.0-alpha.12"
COMMIT = "1" * 40
OTHER_COMMIT = "2" * 40
TAG_OBJECT = "3" * 40
WORKFLOW = f"{REPO}/.github/workflows/release-vendor.yml"
FILES = [
    f"vtium-{VERSION}.tar.gz",
    f"vtium-{VERSION}.zip",
    f"vtium-{VERSION}-vendor.tar.zst",
    f"vtium-{VERSION}-vendor.tar.zst.sha256",
    f"vtium-{VERSION}-dependencies.tsv",
]

# The stand-in for gh. It reads the release from $GH_STUB.
GH_STUB = textwrap.dedent(
    """\
    #!/usr/bin/env python3
    import hashlib, json, os, shutil, sys
    from pathlib import Path

    state = json.loads(Path(os.environ["GH_STUB"]).read_text())
    args = sys.argv[1:]

    def flag(name):
        return args[args.index(name) + 1] if name in args else None

    def fail(message):
        print(message, file=sys.stderr)
        sys.exit(1)

    if args[0] == "api":
        path = args[1]
        if state.get("api_error"):
            fail("HTTP 502: Bad Gateway")
        prefix = "repos/%s/git/ref/tags/" % state["repo"]
        if path.startswith(prefix):
            ref = state["refs"].get(path[len(prefix):])
            if ref is None:
                fail("HTTP 404: Not Found")
            print(json.dumps({"object": ref}))
            sys.exit(0)
        prefix = "repos/%s/git/tags/" % state["repo"]
        if path.startswith(prefix):
            tag = state["tags"].get(path[len(prefix):])
            if tag is None:
                fail("HTTP 404: Not Found")
            print(json.dumps(tag))
            sys.exit(0)
        fail("stub: unknown api path " + path)

    if args[:2] == ["release", "view"]:
        release = state["releases"].get(args[2])
        if release is None:
            fail("release not found")
        names = sorted(p.name for p in Path(release["dir"]).iterdir())
        print(json.dumps({"assets": [{"name": n} for n in names], "isDraft": release["draft"]}))
        sys.exit(0)

    if args[:2] == ["release", "download"]:
        release = state["releases"].get(args[2])
        if release is None:
            fail("release not found")
        if flag("--pattern") != "*":
            fail("stub: the script downloads every file")
        files = sorted(Path(release["dir"]).iterdir())
        if not files:
            fail("no assets to download")
        for p in files:
            shutil.copy(p, flag("--dir") or ".")
        sys.exit(0)

    if args[:2] == ["attestation", "verify"]:
        digest = hashlib.sha256(Path(args[2]).read_bytes()).hexdigest()
        found = [a for a in state["attestations"] if a["digest"] == digest and a["repo"] == flag("--repo")]
        if not found:
            fail("HTTP 404: no attestations found for sha256:" + digest)
        def matches(a):
            if flag("--signer-workflow") is not None and a["workflow"] != flag("--signer-workflow"):
                return False
            if flag("--source-ref") is not None and a["ref"] != flag("--source-ref"):
                return False
            if flag("--source-digest") is not None and a["commit"] != flag("--source-digest"):
                return False
            if "--deny-self-hosted-runners" in args and a["self_hosted"]:
                return False
            return True
        if not any(matches(a) for a in found):
            fail("verification failed: no attestation matches the policy")
        print("verified")
        sys.exit(0)

    fail("stub: unknown command " + " ".join(args))
    """
)


class ReleaseCheck(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        bin_dir = self.tmp / "bin"
        bin_dir.mkdir()
        stub = bin_dir / "gh"
        stub.write_text(GH_STUB)
        stub.chmod(0o755)
        self.assets = self.tmp / "assets"
        self.assets.mkdir()
        for name in FILES:
            (self.assets / name).write_bytes(f"the contents of {name}\n".encode())
        self.write_sums()
        self.state = {
            "repo": REPO,
            "refs": {TAG: {"type": "tag", "sha": TAG_OBJECT}},
            "tags": {TAG_OBJECT: {"object": {"type": "commit", "sha": COMMIT}, "verification": {"verified": True, "reason": "valid"}}},
            "releases": {TAG: {"dir": str(self.assets), "draft": True}},
            "attestations": [],
        }
        self.attest_all()
        self.env = dict(os.environ, PATH=f"{bin_dir}{os.pathsep}{os.environ['PATH']}", GH_STUB=str(self.tmp / "state.json"), GITHUB_REPOSITORY=REPO)
        self.env.pop("GITHUB_ACTIONS", None)

    def tearDown(self):
        self._tmp.cleanup()

    def write_sums(self):
        names = sorted(p.name for p in self.assets.iterdir() if p.name != "SHA256SUMS")
        lines = [f"{hashlib.sha256((self.assets / n).read_bytes()).hexdigest()}  {n}" for n in names]
        (self.assets / "SHA256SUMS").write_text("\n".join(lines) + "\n")

    def attestation(self, path, **over):
        a = {"digest": hashlib.sha256(path.read_bytes()).hexdigest(), "repo": REPO, "workflow": WORKFLOW, "ref": f"refs/tags/{TAG}", "commit": COMMIT, "self_hosted": False}
        a.update(over)
        return a

    def attest_all(self, **over):
        self.state["attestations"] = [self.attestation(p, **over) for p in sorted(self.assets.iterdir())]

    def run_check(self, command, stage="draft", commit=COMMIT):
        args = [TAG] if command == "is-draft" else [stage, TAG, commit]
        (self.tmp / "state.json").write_text(json.dumps(self.state))
        return subprocess.run([sys.executable, "-I", str(SCRIPT), command, *args], env=self.env, capture_output=True, text=True)

    def assert_fails(self, done, *phrases):
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        for phrase in phrases:
            self.assertIn(phrase, done.stderr)

    # The release.

    def test_a_draft_release_passes(self):
        done = self.run_check("is-draft")
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_a_published_release_fails_and_says_how_to_recover(self):
        self.state["releases"][TAG]["draft"] = False
        self.assert_fails(self.run_check("is-draft"), "is already published", f"gh release edit {TAG} --draft=true", "a new release with a new version")

    def test_a_tag_with_no_release_fails_and_says_how_to_save_one(self):
        del self.state["releases"][TAG]
        self.assert_fails(self.run_check("is-draft"), "release not found", f"gh release create {TAG} --draft --verify-tag")

    # The tag.

    def test_a_signed_annotated_tag_at_the_run_commit_passes(self):
        done = self.run_check("tag")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("GitHub verifies its signature", done.stdout)

    def test_a_lightweight_tag_fails(self):
        self.state["refs"][TAG] = {"type": "commit", "sha": COMMIT}
        self.assert_fails(self.run_check("tag"), "lightweight tag", "git push --force origin " + TAG)

    def test_a_tag_github_does_not_verify_fails(self):
        self.state["tags"][TAG_OBJECT]["verification"] = {"verified": False, "reason": "unsigned"}
        self.assert_fails(self.run_check("tag"), "does not verify the signature", "reason unsigned")

    def test_a_tag_of_a_tag_fails(self):
        self.state["tags"][TAG_OBJECT]["object"] = {"type": "tag", "sha": OTHER_COMMIT}
        self.assert_fails(self.run_check("tag"), "names a tag, not a commit")

    def test_a_tag_that_moved_after_the_run_started_fails(self):
        self.state["tags"][TAG_OBJECT]["object"]["sha"] = OTHER_COMMIT
        self.assert_fails(self.run_check("tag"), "The tag moved after the run started", "Start the draft run again")

    def test_a_tag_gh_cannot_read_fails_and_says_so(self):
        self.state["api_error"] = True
        self.assert_fails(self.run_check("tag"), "HTTP 502", "gh could not read the tag")

    def test_a_published_tag_failure_tells_how_to_recover(self):
        self.state["refs"][TAG] = {"type": "commit", "sha": COMMIT}
        self.assert_fails(self.run_check("tag", "published"), f"gh release edit {TAG} --draft=true", "a new release with a new version")

    # The files.

    def test_the_whole_set_passes_on_a_draft_and_once_published(self):
        for stage in ("draft", "published"):
            done = self.run_check("files", stage)
            self.assertEqual(done.returncode, 0, done.stderr)
            self.assertIn("carries exactly the files", done.stdout)

    def test_an_extra_file_fails(self):
        (self.assets / "extra.txt").write_text("extra\n")
        self.state["attestations"].append(self.attestation(self.assets / "extra.txt"))
        self.assert_fails(self.run_check("files"), "does not list: extra.txt", "gh release delete-asset")

    def test_a_missing_file_fails(self):
        (self.assets / FILES[1]).unlink()
        self.assert_fails(self.run_check("files"), f"lacks these files that its SHA256SUMS lists: {FILES[1]}")

    def test_a_corrupted_file_fails(self):
        (self.assets / FILES[4]).write_text("corrupted\n")
        self.assert_fails(self.run_check("files"), f"The file {FILES[4]} of the draft release {TAG} does not match its SHA-256")

    def test_a_corrupted_file_with_sums_to_match_fails_on_its_attestation(self):
        (self.assets / FILES[4]).write_text("corrupted\n")
        self.write_sums()
        self.assert_fails(self.run_check("files"), "no attestations found", f"for these files of the draft release {TAG}: {FILES[4]}, SHA256SUMS.")

    def test_a_release_with_no_files_fails(self):
        for p in self.assets.iterdir():
            p.unlink()
        self.assert_fails(self.run_check("files", "published"), "was published without its files", "--draft=true")
        self.assert_fails(self.run_check("files", "draft"), "carries no files")

    def test_a_release_with_no_sums_fails(self):
        (self.assets / "SHA256SUMS").unlink()
        self.assert_fails(self.run_check("files"), "carries no SHA256SUMS")

    def test_a_listed_name_without_the_version_fails(self):
        os.rename(self.assets / FILES[0], self.assets / "vtium-0.1.0-alpha.11.tar.gz")
        self.write_sums()
        self.attest_all()
        self.assert_fails(self.run_check("files"), "lists vtium-0.1.0-alpha.11.tar.gz, which does not carry the release's version 0.1.0-alpha.12")

    # Each attestation flag rules out one kind of attestation.

    def test_an_attestation_from_a_dry_run_on_a_branch_does_not_count(self):
        self.attest_all(ref="refs/heads/main")
        self.assert_fails(self.run_check("files"), "no attestation matches")

    def test_an_attestation_of_another_commit_does_not_count(self):
        self.attest_all(commit=OTHER_COMMIT)
        self.assert_fails(self.run_check("files"), "no attestation matches")

    def test_an_attestation_from_another_workflow_does_not_count(self):
        self.attest_all(workflow=f"{REPO}/.github/workflows/other.yml")
        self.assert_fails(self.run_check("files"), "no attestation matches")

    def test_an_attestation_from_a_self_hosted_runner_does_not_count(self):
        self.attest_all(self_hosted=True)
        self.assert_fails(self.run_check("files"), "no attestation matches")

    def test_sha256sums_needs_an_attestation_of_its_own(self):
        self.state["attestations"] = [a for a in self.state["attestations"] if a["digest"] != hashlib.sha256((self.assets / "SHA256SUMS").read_bytes()).hexdigest()]
        self.assert_fails(self.run_check("files"), f"for these files of the draft release {TAG}: SHA256SUMS.")

    def test_a_dry_run_attestation_beside_the_release_one_still_passes(self):
        self.state["attestations"] += [self.attestation(p, ref="refs/heads/main", commit=OTHER_COMMIT) for p in sorted(self.assets.iterdir())]
        self.assertEqual(self.run_check("files").returncode, 0)

    def test_under_github_actions_each_failure_is_an_error_annotation(self):
        (self.assets / "extra.txt").write_text("extra\n")
        self.env["GITHUB_ACTIONS"] = "true"
        done = self.run_check("files")
        self.assertEqual(done.returncode, 1)
        self.assertIn("::error::The draft release", done.stdout)

    def test_a_short_commit_is_refused(self):
        done = self.run_check("tag", commit="373d0cd")
        self.assertEqual(done.returncode, 2)


class WorkflowWiring(unittest.TestCase):
    """The workflow calls each check from the job that needs it. The jobs
    themselves run only on GitHub, so this reads the workflow's text."""

    @classmethod
    def setUpClass(cls):
        text = (SCRIPT.parent.parent.parent / ".github/workflows/release-vendor.yml").read_text()
        body = text[text.index("\njobs:\n"):]
        parts = re.split(r"^  ([a-z]+):\n", body, flags=re.M)
        cls.jobs = dict(zip(parts[1::2], parts[2::2]))

    def assert_runs(self, job, *lines):
        for line in lines:
            self.assertIn(line, self.jobs[job], f"the {job} job does not run {line}")

    def test_the_draft_job_checks_the_release_and_the_tag(self):
        self.assert_runs("draft", 'tools/release/check is-draft "$RELEASE_TAG"', 'tools/release/check tag draft "$RELEASE_TAG" "$GITHUB_SHA"')

    def test_a_draft_run_builds_the_commit_it_started_on(self):
        self.assert_runs("archive", "ref: ${{ inputs.draft && github.sha || inputs.ref || github.ref }}")

    def test_the_attach_job_checks_the_whole_draft_after_the_upload(self):
        job = self.jobs["attach"]
        self.assert_runs("attach", 'source/tools/release/check is-draft "$RELEASE_TAG"')
        upload = job.index("gh release upload")
        self.assertGreater(job.index('source/tools/release/check files draft "$RELEASE_TAG" "$GITHUB_SHA"'), upload)
        self.assertIn("attestations: read", job)

    def test_the_published_job_checks_the_tag_and_the_files(self):
        self.assert_runs("published", 'tools/release/check tag published "$TAG" "$GITHUB_SHA"', 'tools/release/check files published "$TAG" "$GITHUB_SHA"')

    def test_each_job_that_runs_the_check_checks_out_the_workflow_commit(self):
        for job in ("draft", "attach", "published"):
            self.assert_runs(job, "ref: ${{ github.sha }}", "sparse-checkout: tools/release", "persist-credentials: false")


if __name__ == "__main__":
    unittest.main()
