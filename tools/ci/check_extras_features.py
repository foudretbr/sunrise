#!/usr/bin/env python3
"""Check that the vanilla (Extras-off) build reuses every dependency the default build compiled, so CI builds the
second variant incrementally: only workspace crates, which own the Extras features, may differ. Compares Cargo's own
plan of each build (`--unit-graph`, which compiles nothing): a dependency unit is reused when its package, target,
mode, features, profile and platform match and so do the units it depends on, as Cargo's fingerprint requires.
Linux, the platform it runs on. `--unit-graph` is unstable: RUSTC_BOOTSTRAP=1 is set for this planning call only."""

import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[2]
BUILD_ROOTS = ("sunrise-game", "sunrise-mod-host")
# The vanilla build: Extras off, and controllers with it (an Extras feature). The release workflows will use this
# same feature set when they build both variants.
EXTRAS_OFF_FEATURES = "audio"


def in_workspace(pkg_id):
    """Whether a unit's package is one of this checkout's path packages (`path+file:///…#version`)."""
    return pkg_id.startswith("path+file://") and Path(unquote(pkg_id[len("path+file://"):].split("#")[0])
                                                      ).is_relative_to(ROOT)


def plan(root, *, extras_off):
    """{key: unit} of every dependency unit outside the workspace, each keyed by what it is built from: itself and,
    recursively, the units it depends on."""
    command = ["cargo", "build", "--locked", "--release", "-p", root, "--unit-graph", "-Z", "unstable-options"]
    if extras_off:
        command += ["--no-default-features", "--features", EXTRAS_OFF_FEATURES]
    units = json.loads(subprocess.check_output(command, cwd=ROOT, text=True,
                                               env={**os.environ, "RUSTC_BOOTSTRAP": "1"}))["units"]
    keys = {}

    def key(index):
        if index not in keys:
            unit = units[index]
            dependencies = sorted((dependency["extern_crate_name"], key(dependency["index"]))
                                  for dependency in unit["dependencies"])
            keys[index] = hashlib.sha256(json.dumps([own(unit), dependencies]).encode()).hexdigest()
        return keys[index]

    return {key(index): unit for index, unit in enumerate(units) if not in_workspace(unit["pkg_id"])}


def own(unit):
    """What a unit is built from, leaving out its dependencies."""
    return json.dumps({name: unit[name] for name in ("pkg_id", "target", "mode", "features", "profile", "platform")},
                      sort_keys=True)


def describe(unit):
    name, version = unit["pkg_id"].rsplit("#", 1)[-1].rsplit("@", 1)
    return f"{name} {version} {unit['target']['name']} ({unit['mode']}; features [{', '.join(unit['features'])}])"


def main():
    failed = False
    for root in BUILD_ROOTS:
        default = plan(root, extras_off=False)
        extras_off = plan(root, extras_off=True)
        # What the vanilla build compiles again after the default build: the units that differ themselves, and how
        # many more are compiled again only because something below them did.
        rebuilt = [unit for key, unit in extras_off.items() if key not in default]
        built = {own(unit) for unit in default.values()}
        causes = sorted({describe(unit) for unit in rebuilt if own(unit) not in built})
        for line in causes:
            print(f"{root}: {line} differs from every unit the default build compiled", file=sys.stderr)
        if rebuilt:
            failed = True
            print(f"{root}: {len(rebuilt)} dependency units would be compiled again for the vanilla build",
                  file=sys.stderr)
    if not failed:
        print("The vanilla build reuses every dependency the default build compiled (sunrise-game and sunrise-mod-host).")
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
