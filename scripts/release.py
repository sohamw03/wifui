#!/usr/bin/env python3
import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
CHOCOLATEY_DIR = PROJECT_ROOT / "choco" / "wifui"
NUSPEC_FILE = CHOCOLATEY_DIR / "wifui.nuspec"
CHOCOLATEY_INSTALL_FILE = CHOCOLATEY_DIR / "tools" / "chocolateyinstall.ps1"
RELEASE_ASSET = "wifui-x86_64-pc-windows-msvc.zip"
RELEASE_URL = "https://github.com/sohamw03/wifui/releases/download/{version}/{asset}"
CHOCO_SOURCE = "https://push.chocolatey.org/"
VERSION_PATTERN = re.compile(
    r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)


def run_command(command, cwd=PROJECT_ROOT):
    """Run a release command and stop if it fails."""
    print(f"Running: {' '.join(command)}")
    try:
        subprocess.run(command, check=True, cwd=cwd)
    except FileNotFoundError:
        print(f"Error: command not found: {command[0]}")
        sys.exit(1)
    except subprocess.CalledProcessError as error:
        print(f"Command failed with exit code {error.returncode}: {' '.join(command)}")
        sys.exit(error.returncode or 1)


def get_cargo_version():
    cargo_file = PROJECT_ROOT / "Cargo.toml"
    if not cargo_file.exists():
        print("Error: Cargo.toml not found in the repository.")
        sys.exit(1)

    content = cargo_file.read_text(encoding="utf-8")
    match = re.search(r'^version\s*=\s*"([^"]+)"', content, re.MULTILINE)
    if not match:
        print("Error: Could not locate package version in Cargo.toml.")
        sys.exit(1)
    return match.group(1)


def confirm(prompt):
    return input(prompt).strip().lower() in ("y", "yes")


def wait_for_manual_step(instruction):
    print(f"\n{instruction}")
    input("Press Enter when this step is complete to continue... ")


def prompt_release_version(default_version=None):
    default_version = default_version or get_cargo_version()
    while True:
        version = input(
            f"Enter release version/tag [current: {default_version}]: "
        ).strip() or default_version
        if VERSION_PATTERN.fullmatch(version):
            return version
        print("Invalid version. Use a SemVer value such as 1.2.3.")


def publish_crates_io():
    cargo_file = PROJECT_ROOT / "Cargo.toml"
    if not cargo_file.exists():
        print("Error: Cargo.toml not found in the repository.")
        sys.exit(1)

    content = cargo_file.read_text(encoding="utf-8")

    # Extract current version
    match = re.search(r'^version\s*=\s*"([^"]+)"', content, re.MULTILINE)
    if not match:
        print("Error: Could not locate package version in Cargo.toml.")
        sys.exit(1)

    current_version = match.group(1)
    release_version = current_version

    # Prompt to update version
    update_choice = (
        input(f"Do you want to update the version? [current: {current_version}] (y/N): ")
        .strip()
        .lower()
    )
    if update_choice in ("y", "yes"):
        new_version = input(f"Enter new version [current: {current_version}]: ").strip()
        if new_version:
            new_content = re.sub(
                r'^version\s*=\s*"[^"]+"',
                f'version = "{new_version}"',
                content,
                count=1,
                flags=re.MULTILINE,
            )
            cargo_file.write_text(new_content, encoding="utf-8")
            release_version = new_version
            print(f"Updated Cargo.toml: {current_version} -> {new_version}")

    # Prompt to publish
    publish_choice = input("Do you want to publish to crates.io? (y/N): ").strip().lower()
    if publish_choice in ("y", "yes"):
        run_command(["cargo", "publish"])

    return release_version


def publish_scoop():
    print("\nChecking Scoop Bucket Excavator status...")
    url = "https://api.github.com/repos/sohamw03/Scoop-Bucket/actions/workflows/excavator.yml"
    req = urllib.request.Request(
        url,
        headers={
            "User-Agent": "Python-Release-Script",
            "Accept": "application/vnd.github+json",
        },
    )
    try:
        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read().decode())
            state = data.get("state")
            name = data.get("name", "Excavator")

            if state == "active":
                print(f"✓ Workflow '{name}' is ENABLED & ACTIVE.")
                print("  Excavator will automatically pick up new release tags.")
            else:
                print(f"⚠️ Workflow '{name}' status is: {state}")
    except urllib.error.HTTPError as e:
        print(f"Failed to fetch workflow status (HTTP {e.code}): {e.reason}")
    except Exception as e:
        print(f"Error checking Excavator status: {e}")


def download_release_zip(url):
    """Download a release asset to a temporary file and return its path."""
    temporary_file = tempfile.NamedTemporaryFile(
        prefix="wifui-release-", suffix=".zip", delete=False
    )
    temporary_path = Path(temporary_file.name)

    try:
        request = urllib.request.Request(
            url,
            headers={"User-Agent": "Python-Release-Script"},
        )
        with temporary_file, urllib.request.urlopen(request) as response:
            while True:
                chunk = response.read(1024 * 1024)
                if not chunk:
                    break
                temporary_file.write(chunk)
    except (urllib.error.HTTPError, urllib.error.URLError, OSError) as error:
        temporary_path.unlink(missing_ok=True)
        print(f"Failed to download release asset: {error}")
        sys.exit(1)

    return temporary_path


def calculate_sha256(file_path):
    digest = hashlib.sha256()
    with file_path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def update_chocolatey_files(version, release_url, checksum):
    """Substitute release metadata in the Chocolatey package templates."""
    nuspec_content = NUSPEC_FILE.read_text(encoding="utf-8")
    nuspec_content, version_count = re.subn(
        r"(<version>)[^<]*(</version>)",
        lambda match: f"{match.group(1)}{version}{match.group(2)}",
        nuspec_content,
        count=1,
    )
    if version_count != 1:
        raise RuntimeError(f"Could not locate package version in {NUSPEC_FILE}.")

    install_content = CHOCOLATEY_INSTALL_FILE.read_text(encoding="utf-8")
    install_content, url_count = re.subn(
        r"(?m)^(\$url64\s*=\s*)['\"][^'\"]+['\"]",
        lambda match: f"{match.group(1)}'{release_url}'",
        install_content,
        count=1,
    )
    if url_count != 1:
        raise RuntimeError(
            f"Could not locate the release URL in {CHOCOLATEY_INSTALL_FILE}."
        )

    install_content, checksum_count = re.subn(
        r"(?m)^(\s*checksum64\s*=\s*)['\"][^'\"]+['\"]",
        lambda match: f"{match.group(1)}'{checksum}'",
        install_content,
        count=1,
    )
    if checksum_count != 1:
        raise RuntimeError(
            f"Could not locate the SHA-256 checksum in {CHOCOLATEY_INSTALL_FILE}."
        )

    NUSPEC_FILE.write_text(nuspec_content, encoding="utf-8")
    CHOCOLATEY_INSTALL_FILE.write_text(install_content, encoding="utf-8")
    print(f"Updated Chocolatey package metadata for version {version}.")


def select_chocolatey_package(version):
    package_path = CHOCOLATEY_DIR / f"wifui.{version}.nupkg"
    if not package_path.exists():
        print(f"Error: choco pack did not create {package_path.name}.")
        sys.exit(1)
    return package_path


def publish_choco(version=None):
    print("\nPublishing to Chocolatey...")
    version = version or prompt_release_version()
    release_url = RELEASE_URL.format(version=version, asset=RELEASE_ASSET)

    print(f"Downloading release asset: {release_url}")
    zip_path = download_release_zip(release_url)
    try:
        checksum = calculate_sha256(zip_path)
    finally:
        zip_path.unlink(missing_ok=True)
    print(f"SHA-256: {checksum}")

    try:
        update_chocolatey_files(version, release_url, checksum)
    except (OSError, RuntimeError) as error:
        print(f"Failed to update Chocolatey package files: {error}")
        sys.exit(1)

    run_command(["choco", "pack"], cwd=CHOCOLATEY_DIR)
    package_path = select_chocolatey_package(version)
    package_name = package_path.name

    print("\nManual Chocolatey validation (run these from choco/wifui):")
    print("  choco install wifui --source .")
    print("  choco uninstall wifui")
    if not confirm("Have install and uninstall both succeeded? (y/N): "):
        print("Chocolatey package was created but not pushed.")
        return

    run_command(
        [
            "choco",
            "push",
            f".\\{package_name}",
            "--source",
            CHOCO_SOURCE,
        ],
        cwd=CHOCOLATEY_DIR,
    )
    print(f"Pushed {package_name} to Chocolatey.")


def publish_winget():
    print("\nPublishing to WinGet...")
    pass


def main():
    parser = argparse.ArgumentParser(description="Rust package publishing helper.")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--crates", "--crates-io", action="store_true", help="Publish to crates.io")
    group.add_argument("--scoop", action="store_true", help="Check Scoop Excavator status")
    group.add_argument("--choco", action="store_true", help="Publish to Chocolatey")
    group.add_argument("--winget", action="store_true", help="Publish to WinGet")

    args = parser.parse_args()

    if args.crates:
        target = "crates.io"
    elif args.scoop:
        target = "scoop"
    elif args.choco:
        target = "choco"
    elif args.winget:
        target = "winget"
    else:
        run_full_release()
        return

    print(f"Selected: {target}\n")

    if target == "crates.io":
        publish_crates_io()
    elif target == "scoop":
        publish_scoop()
    elif target == "choco":
        publish_choco()
    elif target == "winget":
        publish_winget()


def run_full_release():
    """Run the interactive release handoff in publishing order."""
    cargo_version = publish_crates_io()
    wait_for_manual_step(
        "Update the application images/GIF as needed and save the changes."
    )
    wait_for_manual_step("Update README.md with the release changes as needed.")

    version = prompt_release_version(cargo_version)
    print(
        f"\nRelease tag/version: {version}\n"
        "The GitHub release asset must be available before the Chocolatey step."
    )
    wait_for_manual_step(
        "Commit, tag, and push the release manually, then wait for its GitHub asset.\n"
        f"  git add .\n"
        f'  git commit -m "release: {version}"\n'
        f"  git tag {version}\n"
        "  git push origin main --tags"
    )

    publish_scoop()
    publish_choco(version)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\nAborted.")
        sys.exit(0)
