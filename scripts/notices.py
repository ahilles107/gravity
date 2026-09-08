#!/usr/bin/env python3
"""Generate notices from frozen installs; check committed inputs offline on CI."""

import argparse
import hashlib
from html.parser import HTMLParser
import json
import pathlib
import re
import subprocess
import tomllib
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
TARGET = "aarch64-apple-darwin"
EXCLUDED = {"other-platform", "build tool (not shipped)"}
INVENTORY = ROOT / "third-party/inventory.json"
OVERRIDES = ROOT / "third-party/overrides"
OUTPUTS = {
    "daemon": "third-party/DAEMON_NOTICES.txt",
    "desktop": "third-party/DESKTOP_NOTICES.txt",
    "marketing": "apps/marketing/public/THIRD_PARTY_NOTICES.txt",
}
INPUTS = [
    "Cargo.lock", "Cargo.toml", "crates/gravityd/Cargo.toml", "crates/bus/Cargo.toml",
    "apps/desktop/src-tauri/Cargo.lock", "apps/desktop/src-tauri/Cargo.toml",
    "apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json",
    "apps/desktop/pnpm-lock.yaml", "apps/desktop/vite.config.ts",
    "apps/marketing/package.json", "apps/marketing/pnpm-lock.yaml",
    "apps/marketing/src/main.ts", "apps/marketing/src/worker.ts",
    "apps/marketing/src/updater-proxy.ts", "apps/marketing/src/updater-bridge.ts",
    "apps/marketing/wrangler.updater-bridge.jsonc",
    ".github/workflows/release.yml", "scripts/prepare-sidecar.sh",
    "scripts/notices.py", "third-party/README.md",
]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def notice_text(data):
    return "\n".join(line.rstrip() for line in data.decode("utf-8").splitlines()).rstrip() + "\n"


def run(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True)


def input_hashes():
    files = [ROOT / name for name in INPUTS] + sorted(OVERRIDES.rglob("*"))
    return {str(p.relative_to(ROOT)): digest(p.read_bytes()) for p in files if p.is_file()}


def license_files(root):
    # Include nested vendored notices (not just the package's top-level license).
    pattern = re.compile(r"^(licen[sc]e|copying|notice|copyright)(?:$|[-_.])", re.I)
    return sorted(p for p in root.rglob("*") if p.is_file()
                  and "node_modules" not in p.relative_to(root).parts
                  and pattern.match(p.name)
                  and p.suffix.lower() not in {".rs", ".js", ".mjs", ".map", ".ts"})


class LicenseHTML(HTMLParser):
    def __init__(self):
        super().__init__()
        self.text = []

    def handle_data(self, data):
        self.text.append(data)


def toolchain_package():
    version = run("rustc", "--version").strip()
    release = version.split()[1]
    root = pathlib.Path(run("rustc", "--print", "sysroot").strip()) / "share/doc/rust"
    files = [root / "COPYRIGHT-library.html"]
    files += [root / "licenses" / (name + ".txt") for name in
              ["MIT", "Apache-2.0", "Unicode-3.0", "LLVM-exception", "BSD-2-Clause"]]
    if not all(file.is_file() for file in files):
        raise ValueError("Install the matching official rustc component to collect standard library notices")
    source = f"https://static.rust-lang.org/dist/rustc-{release}-src.tar.xz"
    copyright_source = f"https://static.rust-lang.org/dist/rustc-{release}-{TARGET}.tar.xz"
    with urllib.request.urlopen(source + ".sha256", timeout=30) as response:
        checksum = response.read().decode().split()[0]
    return {"ecosystem": "Rust toolchain", "name": "standard-library", "version": version,
            "license": "MIT OR Apache-2.0; upstream component exceptions below",
            "components": {"daemon": "runtime", "desktop": "runtime"},
            "source": source, "checksum": checksum, "copyright_source": copyright_source,
            "path": root, "files": files}


def rust_packages(manifest, component):
    data = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1",
                         "--manifest-path", manifest))
    filtered = json.loads(run("cargo", "metadata", "--locked", "--format-version", "1",
                             "--filter-platform", TARGET, "--manifest-path", manifest))
    selected = {node["id"] for node in filtered["resolve"]["nodes"]}
    runtime = run("cargo", "tree", "--locked", "--target", TARGET,
                  "--manifest-path", manifest, "--edges", "normal,no-proc-macro",
                  "--prefix", "none", "--format", "{p}").splitlines()
    runtime = {line.removesuffix(" (*)") for line in runtime}
    lock = tomllib.loads((ROOT / manifest).with_name("Cargo.lock").read_text())
    checksums = {(p["name"], p["version"]): p.get("checksum") for p in lock["package"]}
    for p in data["packages"]:
        if p["source"] is None:
            continue
        if not p["source"].startswith("registry+"):
            raise ValueError("Review non-registry source: " + p["name"])
        scope = "other-platform"
        if p["id"] in selected:
            scope = "runtime" if f"{p['name']} v{p['version']}" in runtime else "build/test"
        yield {
            "ecosystem": "Rust", "name": p["name"], "version": p["version"],
            "license": p["license"], "components": {component: scope},
            "source": f"https://static.crates.io/crates/{p['name']}/{p['name']}-{p['version']}.crate",
            "checksum": checksums[p["name"], p["version"]],
            "path": pathlib.Path(p["manifest_path"]).parent,
        }


def js_packages():
    data = json.loads(run("pnpm", "--dir", "apps/desktop", "licenses", "list", "--prod", "--json"))
    for license_name, rows in data.items():
        for row in rows:
            for version, path in zip(row["versions"], row["paths"], strict=True):
                name = row["name"]
                scope = "build/types" if name.startswith("@types/") or name in {"csstype", "@posthog/types"} else "runtime dependency closure"
                yield {"ecosystem": "JavaScript", "name": name, "version": version,
                       "license": license_name, "components": {"desktop": scope},
                       "source": f"https://registry.npmjs.org/{name}/-/{name.split('/')[-1]}-{version}.tgz",
                       "path": pathlib.Path(path)}
    path = (ROOT / "apps/desktop/node_modules/vite").resolve()
    package = json.loads((path / "package.json").read_text())
    yield {"ecosystem": "JavaScript", "name": "vite", "version": package["version"],
           "license": package["license"], "components": {"desktop": "generated runtime helper"},
           "source": f"https://registry.npmjs.org/vite/-/vite-{package['version']}.tgz", "path": path}
    # Vite injects its modulepreload helper; the other marketing tools are not shipped.
    marketing = json.loads(run("pnpm", "--dir", "apps/marketing", "licenses", "list", "--json"))
    for license_name, rows in marketing.items():
        for row in rows:
            for version, path in zip(row["versions"], row["paths"], strict=True):
                name = row["name"]
                scope = "generated runtime helper" if name == "vite" else "build tool (not shipped)"
                yield {"ecosystem": "JavaScript", "name": name, "version": version,
                       "license": license_name, "components": {"marketing": scope},
                       "source": f"https://registry.npmjs.org/{name}/-/{name.split('/')[-1]}-{version}.tgz",
                       "path": pathlib.Path(path)}


def generate():
    overrides = json.loads((OVERRIDES / "index.json").read_text())
    packages = {}
    for p in [*rust_packages("Cargo.toml", "daemon"),
              *rust_packages("apps/desktop/src-tauri/Cargo.toml", "desktop"), *js_packages(),
              toolchain_package()]:
        key = (p["ecosystem"], p["name"], p["version"])
        if key in packages:
            packages[key]["components"].update(p["components"])
        else:
            packages[key] = p
    sections = {component: [] for component in OUTPUTS}
    records = []
    for key, p in sorted(packages.items()):
        root = p.pop("path")
        files = p.pop("files", None)
        included = any(scope not in EXCLUDED for scope in p["components"].values())
        texts = []
        p["notices"] = []
        if included:
            for file in files if files is not None else license_files(root):
                data = file.read_bytes()
                name = str(file.relative_to(root))
                text = notice_text(data)
                if file.suffix == ".html":
                    parser = LicenseHTML()
                    parser.feed(text)
                    text = re.sub(r"\n{3,}", "\n\n", notice_text("".join(parser.text).encode()))
                texts.append(f"--- {name} ---\n" + text)
                p["notices"].append({"path": name, "sha256": digest(data)})
            for extra in overrides.get(p["name"] + "@" + p["version"], []):
                data = (OVERRIDES / extra["file"]).read_bytes()
                texts.append(f"--- {extra['url']} ---\n" + notice_text(data))
                p["notices"].append({"url": extra["url"], "sha256": digest(data),
                                     "upstream_sha256": extra["upstream_sha256"]})
            if not texts:
                raise ValueError("Missing authentic notice text: " + " ".join(key))
            header = ("\n" + "=" * 72 + f"\n{p['ecosystem']}: {p['name']} {p['version']}\n"
                      + f"Declared license: {p['license']}\nSource: {p['source']}\n")
            if p.get("checksum"):
                header += "Source archive SHA-256: " + p["checksum"] + "\n"
            if p["license"] == "MPL-2.0":
                header += ("Unmodified source is available at the Source URL above under MPL-2.0.\n"
                           "You may obtain and modify that source under the MPL-2.0 terms below.\n")
            if p["ecosystem"] == "Rust toolchain":
                header += ("Official library copyright inventory: " + p["copyright_source"] + "\n"
                           "Includes upstream platform/build dependencies conservatively.\n"
                           "Unmodified third-party sources are available from each versioned crate URL below.\n"
                           "MPL components are available in source form under their included MPL terms.\n")
            for component in OUTPUTS:
                scope = p["components"].get(component)
                scopes = [scope] if scope and scope not in EXCLUDED else []
                daemon_scope = p["components"].get("daemon")
                if component == "desktop" and daemon_scope and daemon_scope not in EXCLUDED:
                    scopes.append("bundled daemon: " + daemon_scope)
                if scopes:
                    sections[component].append(header + "Scope: " + "; ".join(scopes) + "\n\n" + "\n".join(texts))
        records.append(p)
    outputs = {}
    for component, name in OUTPUTS.items():
        content = (f"GRAVITY {component.upper()} THIRD-PARTY NOTICES\n\n"
                   "Gravity's MIT license is separate from these upstream terms.\n"
                   f"Native inventory target: {TARGET}.\n"
                   "Runtime dependency closures are conservative; tree-shaken code may be absent.\n"
                   "Build/test notices are included conservatively for generated code; their\n"
                   "presence does not mean the tools themselves are distributed.\n"
                   "Source URLs identify the exact unmodified dependency versions.\n"
                   "See third-party/README.md in the Gravity source for scope and maintenance.\n"
                   + "".join(sections[component]))
        path = ROOT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content.rstrip() + "\n")
        outputs[name] = digest(path.read_bytes())
    INVENTORY.write_text(json.dumps({"target": TARGET, "inputs": input_hashes(),
                                    "outputs": outputs, "packages": records}, indent=2) + "\n")
    print(f"Generated {len(records)} inventory records and {len(outputs)} notice bundles.")


def check(check_toolchain=False):
    inventory = json.loads(INVENTORY.read_text())
    if inventory["inputs"] != input_hashes():
        raise ValueError("Notice inputs changed. Review scope and run pnpm notices:generate.")
    for name, expected in inventory["outputs"].items():
        if digest((ROOT / name).read_bytes()) != expected:
            raise ValueError("Notice bundle changed: " + name)
    if set(inventory["outputs"]) != set(OUTPUTS.values()):
        raise ValueError("Notice output inventory is incomplete")
    for p in inventory["packages"]:
        if any(scope not in EXCLUDED for scope in p["components"].values()) and not p["notices"]:
            raise ValueError("Missing notices: " + p["name"])
        if check_toolchain and p["ecosystem"] == "Rust toolchain" and p["version"] != run("rustc", "--version").strip():
            raise ValueError("Rust toolchain changed. Review and regenerate standard library notices.")
    print("Dependency notice inputs and bundles match the reviewed inventory.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--generate", action="store_true")
    parser.add_argument("--check-toolchain", action="store_true")
    args = parser.parse_args()
    if args.generate:
        generate()
    else:
        check(args.check_toolchain)
