#!/usr/bin/env python3
"""
Update CHANGELOG.md with a new release section.

Usage:
    scripts/update-changelog.py <tag> <release_date> [changelog_text_or_file]
"""

import sys
import os

def main():
    if len(sys.argv) < 3:
        print("Usage: update-changelog.py <tag> <release_date> [changelog_body]", file=sys.stderr)
        sys.exit(1)

    tag = sys.argv[1].strip()
    date = sys.argv[2].strip()
    body = sys.argv[3].strip() if len(sys.argv) > 3 else ""

    if not body:
        body = "* Maintenance release and automated dependency updates."

    changelog_path = os.path.join(os.path.dirname(__file__), "..", "CHANGELOG.md")
    changelog_path = os.path.abspath(changelog_path)

    if os.path.exists(changelog_path):
        with open(changelog_path, "r", encoding="utf-8") as f:
            content = f.read()
    else:
        content = "# Changelog\n\nAll notable changes to this project will be documented in this file.\n\n## [Unreleased]\n\n"

    # Avoid duplicate section if tag already recorded
    target_header = f"## [{tag}]"
    if target_header in content:
        print(f"Section {target_header} already exists in CHANGELOG.md. Skipping.")
        sys.exit(0)

    entry = f"## [{tag}] - {date}\n\n{body}\n\n"

    unreleased_marker = "## [Unreleased]"
    if unreleased_marker in content:
        idx = content.find(unreleased_marker) + len(unreleased_marker)
        new_content = content[:idx] + "\n\n" + entry + content[idx:].lstrip()
    else:
        new_content = "# Changelog\n\n" + entry + content

    with open(changelog_path, "w", encoding="utf-8") as f:
        f.write(new_content)

    print(f"Successfully updated CHANGELOG.md for {tag} ({date})")

if __name__ == "__main__":
    main()
