"""Linux local CI. Run from the workspace root with the pinned tools on PATH."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import tomllib


def run(*cmd, env=None, timeout=900):
    with log.open("ab") as stream:
        result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=stream,
                                env=env, timeout=timeout, check=False)
        stream.write(result.stdout)
    if result.returncode:
        raise SystemExit(f"{cmd[0]} {cmd[1]} failed ({result.returncode}). See {log}")
    return result.stdout.decode()


def git_paths(*args):
    return {Path(x) for x in run("git", *args, "-z").split("\0") if x}


def is_doc(path):
    return path == Path("README.md") or (path.parts[0] == "docs" and path.suffix == ".md")


def setup():
    commands = {"gitleaks": ["gitleaks", "version"], "clippy": ["cargo", "clippy", "--version"]}
    for tool, version in tomllib.loads(Path("tools.lock.toml").read_text())["versions"].items():
        line = run(*commands.get(tool, [tool, "--version"])).splitlines()[0]
        if version not in line.split():
            raise SystemExit(f"expected {tool} {version}, found {line}")


def affected(packages, changed):
    owners = set()
    for path in changed:
        if is_doc(path):
            continue
        owner = next((name for name, (root, _) in packages.items() if path.is_relative_to(root)), None)
        if owner is None or path.name == "Cargo.toml":
            return set(packages)
        owners.add(owner)
    return {name for name in packages if closure(packages, name) & owners}


def closure(packages, name):
    result = {name}
    for dep in packages[name][1]:
        result.update(closure(packages, dep))
    return result


def package_suite(name, inputs, config, command, env):
    start = time.monotonic()
    digest = hashlib.sha256(json.dumps([name, command, config], sort_keys=True).encode())
    for path in sorted(inputs):
        digest.update(os.fsencode(path) + b"\0" + hashlib.sha256(path.read_bytes()).digest())
    entry = cache / digest.hexdigest()
    reused = entry.exists() and not args.full
    if reused:
        count = int(entry.read_text())
    else:
        output = run(*command, env=env)
        count = sum(map(int, re.findall(r"^test result: ok\. (\d+) passed", output, re.M)))
        with tempfile.NamedTemporaryFile(dir=cache, mode="w", delete=False) as tmp:
            tmp.write(str(count))
        os.replace(tmp.name, entry)
    return dict(package=name, passed=count, reused=reused, seconds=round(time.monotonic()-start, 3))


def check():
    sha = run("git", "rev-parse", "HEAD").strip()
    base = run("git", "merge-base", args.base, "HEAD").strip()
    changed = git_paths("diff", "--name-only", "--no-renames", base) | git_paths("ls-files", "--others", "--exclude-standard")
    paths = {p for p in git_paths("ls-files", "--cached", "--others", "--exclude-standard") if p.is_file()}
    patch = run("git", "diff", "--no-ext-diff", "--unified=0", base)
    additions = [line[1:] for line in patch.splitlines() if line.startswith("+") and not line.startswith("+++")]
    for path in git_paths("ls-files", "--others", "--exclude-standard"):
        additions.extend(path.read_bytes().decode(errors="replace").splitlines())
    if any(re.search(r"(?:^\s*(?:[-*]\s+)?|//\s*|#\s*|/\*\s*)TO" r"DO\b|\bTO" r"DO\s*[:(]", line) for line in additions):
        raise SystemExit("Changed lines contain an unresolved task marker")
    run("gitleaks", "git", "--redact", "--no-banner", "--log-opts", f"{base}..HEAD")
    # Scan the prospective tree, including working changes and excluding local records.
    with tempfile.TemporaryDirectory() as scratch:
        for path in paths:
            dest = Path(scratch) / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(path.read_bytes())
        run("gitleaks", "dir", "--redact", "--no-banner", scratch)

    if not args.full and changed and all(map(is_doc, changed)):
        print(json.dumps(dict(commit=sha, path="text-only")))
        return
    meta = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1"))
    members = {p["id"]: p for p in meta["packages"] if p["id"] in meta["workspace_members"]}
    packages = {}
    for node in meta["resolve"]["nodes"]:
        if node["id"] in members:
            pkg = members[node["id"]]
            root = Path(pkg["manifest_path"]).parent.relative_to(Path.cwd())
            deps = {members[d]["name"] for d in node["dependencies"] if d in members}
            packages[pkg["name"]] = root, deps
    selected = set(packages) if args.full or not changed else affected(packages, changed)
    run("cargo", "fmt", "--all", "--check")
    run("cargo", "clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings")
    run("cargo", "deny", "--locked", "check", "advisories", "licenses")
    host = re.search(r"^host: (.+)$", run("rustc", "-vV"), re.M)[1]
    env = dict(os.environ)
    env["CARGO_TARGET_" + host.replace("-", "_").upper() + "_RUNNER"] = "timeout --kill-after=5s 60s"
    shared = {p for p in paths if not is_doc(p) and not any(p.is_relative_to(v[0]) for v in packages.values())}
    shared.update(p for p in paths if p.name == "Cargo.toml")
    reports = []
    for name in sorted(selected):
        roots = [packages[dep][0] for dep in closure(packages, name)]
        inputs = shared | {p for p in paths if any(p.is_relative_to(r) for r in roots)}
        cmd = ["cargo", "test", "--locked", "-p", name, "--target", host]
        config = [host, "default features", sha, {k:v for k,v in env.items() if k.startswith(("CARGO_", "RUST"))}]
        reports.append(package_suite(name, inputs, config, cmd, env))
    print(json.dumps(dict(commit=sha, packages=reports, checks="pass")))


if __name__ == "__main__":
    cli = argparse.ArgumentParser()
    cli.add_argument("--base", default="origin/main")
    cli.add_argument("--setup", action="store_true")
    cli.add_argument("--full", action="store_true")
    args = cli.parse_args()
    cache = Path("target/local-ci/cache")
    cache.mkdir(parents=True, exist_ok=True)
    log = Path(tempfile.mkstemp(dir=cache.parent, prefix="run-", suffix=".log")[1])
    os.environ.setdefault("CARGO_BUILD_JOBS", "2")
    setup()
    if not args.setup:
        check()
