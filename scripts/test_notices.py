"""The offline gate must reject stale dependencies and damaged distribution files."""

import copy
import json
import unittest
from unittest.mock import patch

import notices


class NoticeChecks(unittest.TestCase):
    def test_current_inventory(self):
        notices.check()

    def test_app_notices_cover_every_bundled_daemon_dependency(self):
        inventory = json.loads(notices.INVENTORY.read_text())
        desktop = (notices.ROOT / notices.OUTPUTS["desktop"]).read_text()
        for package in inventory["packages"]:
            scope = package["components"].get("daemon")
            if scope and scope not in notices.EXCLUDED:
                header = f"{package['ecosystem']}: {package['name']} {package['version']}\n"
                self.assertTrue(header in desktop, "Missing bundled dependency: " + header)

    def test_new_toolchain_requires_notice_review(self):
        with patch.object(notices, "run", return_value="rustc changed"):
            with self.assertRaisesRegex(ValueError, "toolchain changed"):
                notices.check(check_toolchain=True)

    def test_changed_lockfile_requires_review(self):
        inputs = notices.input_hashes()
        inputs["Cargo.lock"] = "changed dependency"
        with patch.object(notices, "input_hashes", return_value=inputs):
            with self.assertRaisesRegex(ValueError, "inputs changed"):
                notices.check()

    def test_damaged_bundle_is_rejected(self):
        inventory = copy.deepcopy(json.loads(notices.INVENTORY.read_text()))
        inventory["outputs"]["third-party/DAEMON_NOTICES.txt"] = "damaged"
        with patch.object(notices.json, "loads", return_value=inventory):
            with self.assertRaisesRegex(ValueError, "bundle changed"):
                notices.check()

    def test_missing_license_is_rejected(self):
        inventory = copy.deepcopy(json.loads(notices.INVENTORY.read_text()))
        package = next(p for p in inventory["packages"] if p["notices"])
        package["notices"] = []
        with patch.object(notices.json, "loads", return_value=inventory):
            with self.assertRaisesRegex(ValueError, "Missing notices"):
                notices.check()
