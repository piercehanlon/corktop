"""Build and install a clean native main revision, or verify its installed binary."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import tempfile
import tomllib


def output(*cmd, env=None):
    return subprocess.check_output(cmd, text=True, env=env, timeout=120).strip()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def notices(dest, target):
    metadata = json.loads(output("cargo", "metadata", "--format-version", "1",
                                 "--locked", "--filter-platform", target))
    rust_docs = Path(output("rustc", "--print", "sysroot")) / "share/doc/rust"
    shutil.copy(rust_docs / "COPYRIGHT-library.html", dest)
    shutil.copytree(rust_docs / "licenses", dest / "rust-licenses")
    shutil.copy("LICENSE", dest / "LICENSE")
    with (dest / "THIRD-PARTY.txt").open("w", encoding="utf-8") as stream:
        for pkg in metadata["packages"]:
            if not pkg["source"]:
                continue
            print(pkg["name"], pkg["version"], pkg["license"], pkg["repository"], file=stream)
            print(", ".join(pkg["authors"]), file=stream)
            root = Path(pkg["manifest_path"]).parent
            for path in sorted(root.rglob("*")):
                if path.is_file() and (re.search(r"licen[sc]e|copying|^notice", path.name, re.I)
                                       or path.parent.name == "fonts" and path.suffix == ".txt"):
                    print(path.relative_to(root), path.read_text(errors="replace"), sep="\n", file=stream)
            print(file=stream)


def install(dest, sha, target):
    if output("git", "status", "--porcelain") or sha != output("git", "rev-parse", "origin/main"):
        raise SystemExit("Install requires a clean checkout at the fetched origin/main")
    versions = tomllib.loads(Path("tools.lock.toml").read_text())["versions"]
    for tool in ("cargo", "rustc"):
        if output(tool, "--version").split()[1] != versions[tool]:
            raise SystemExit(f"Check the pinned {tool} toolchain")
    subprocess.run(["cargo", "build", "--release", "--locked", "-j", "2", "--target",
                    target, "--target-dir", "target", "-p", "corktop"], check=True)
    binary = Path("target") / target / "release" / exe
    identity = json.loads(output(binary, "--identity"))
    if identity["commit"] != sha or identity["target"] != target:
        raise SystemExit("Release identity mismatch")
    dest.mkdir(parents=True, exist_ok=True)
    release = dest / f"{identity['version']}-{sha}"
    if not release.exists():
        with tempfile.TemporaryDirectory(dir=dest) as scratch:
            stage = Path(scratch) / "install"
            stage.mkdir()
            shutil.copy2(binary, stage / exe)
            notices(stage, target)
            if digest(stage / exe) != digest(binary):
                raise SystemExit("Artifact digest mismatch")
            stage.rename(release)
    if digest(release / exe) != digest(binary):
        raise SystemExit("Existing install differs from the release build")
    print(json.dumps(dict(version=identity["version"], commit=sha, target=target, sha256=digest(binary))))
    return release / exe


def verify(binary, sha, target):
    if not binary.is_absolute():
        raise SystemExit("Supply the absolute path to the installed executable")
    artifact = Path("target") / target / "release" / exe
    checksum = digest(binary)
    if binary.samefile(artifact) or checksum != digest(artifact):
        raise SystemExit("Expected a separate installation matching the built artifact")
    with tempfile.TemporaryDirectory() as scratch:
        root = Path(scratch)
        env = dict(os.environ)
        for var in ("HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "XDG_CONFIG_HOME",
                    "XDG_CACHE_HOME", "XDG_DATA_HOME", "TMPDIR", "TEMP", "TMP"):
            path = root / var.lower()
            path.mkdir()
            env[var] = str(path)
        def invoke(*args):
            return json.loads(output(binary, *args, env=env))
        identity = invoke("--identity")
        version = output(binary, "--version", env=env)
        if identity["commit"] != sha or identity["target"] != target or identity["version"] != version:
            raise SystemExit("Installed identity mismatch")
        if invoke("--health") != {"health": "pass"} or invoke("--ready") != {"readiness": "pass"}:
            raise SystemExit("Installed self-check failed")
        board = root / "board"
        marker = "corktop-gate-2-fixture"
        first = invoke("--probe", board)
        def snapshot():
            return {p.name: p.read_bytes() for p in board.iterdir()}
        state = snapshot()
        expected = {"schema_version": 1, "marker": marker}
        if (first != expected or tomllib.loads(state[".board.toml"].decode()) != expected
                or invoke("--probe", board) != first or snapshot() != state):
            raise SystemExit("Installed persistence check failed")
        # Exercise an independently seeded old manifest in this verifier's disposable board.
        (board / ".board.toml").write_text(f'schema_version=0\nmarker="{marker}"\nextra=37\n', encoding="utf-8")
        if invoke("--probe", board) != first:
            raise SystemExit("Forward migration failed")
        migrated = snapshot()
        if (tomllib.loads(migrated[".board.toml"].decode()) != dict(expected, extra=37)
                or invoke("--probe", board) != first or snapshot() != migrated):
            raise SystemExit("Forward migration lost data or repeated a write")
        with subprocess.Popen([binary], env=env) as window:
            try:
                response = input(f"Inspect Corktop. If the borderless window shows only {version}, type its displayed version: ")
                if window.poll() is not None or response.strip() != version:
                    raise SystemExit("Window verification incomplete")
            finally:
                if window.poll() is None:
                    window.terminate()
        print(json.dumps(dict(**identity, sha256=checksum, health="pass", readiness="pass",
                              window="version-only borderless", migration="pass", repeat_probe="pass")))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--install", type=Path, metavar="VERSION_DIRECTORY")
    action.add_argument("--verify", type=Path, metavar="INSTALLED_BINARY")
    args = parser.parse_args()
    exe = "corktop.exe" if os.name == "nt" else "corktop"
    sha = output("git", "rev-parse", "HEAD")
    if sha != output("git", "rev-parse", "origin/main"):
        raise SystemExit("Check out the fetched main revision")
    target = re.search(r"^host: (.+)$", output("rustc", "-vV"), re.M)[1]
    if (platform.system(), target) not in {("Windows", "x86_64-pc-windows-msvc"),
                                           ("Linux", "x86_64-unknown-linux-gnu")}:
        raise SystemExit("Use the native Linux GNU or Windows MSVC x86-64 host")
    installed = install(args.install.expanduser().resolve(), sha, target) if args.install else args.verify
    verify(installed, sha, target)
