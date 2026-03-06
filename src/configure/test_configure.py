"""Unit tests for the Configure tool and the config utility module."""

import json
import os
import tempfile
import unittest
from unittest.mock import patch

from fastmcp import FastMCP

from configure import Configure
from configure.helpers import get_listable_options
from utils.config import (
    CONFIG_OPTIONS,
    _snapshot_client_env_vars,
    apply_config_to_env,
    delete_config_value,
    get_config_value,
    get_effective_value,
    load_config,
    mask_sensitive_value,
    save_config,
    set_config_value,
)


class _ConfigDirMixin:
    """Set CS_CONFIG_DIR to a fresh temp directory for each test."""

    def setUp(self):
        self._tmpdir = tempfile.mkdtemp(prefix="cs_mcp_config_test_")
        self._original_env = os.environ.copy()
        os.environ["CS_CONFIG_DIR"] = self._tmpdir
        _snapshot_client_env_vars()

    def tearDown(self):
        os.environ.clear()
        os.environ.update(self._original_env)
        _snapshot_client_env_vars()
        import shutil

        shutil.rmtree(self._tmpdir, ignore_errors=True)


# --- Config utility tests ---


class TestLoadAndSave(_ConfigDirMixin, unittest.TestCase):
    def test_load_returns_empty_when_no_file(self):
        self.assertEqual(load_config(), {})

    def test_round_trip(self):
        save_config({"access_token": "tok123"})
        self.assertEqual(load_config(), {"access_token": "tok123"})

    def test_load_returns_empty_on_malformed_json(self):
        config_path = os.path.join(self._tmpdir, "config.json")
        with open(config_path, "w") as f:
            f.write("{invalid json")
        self.assertEqual(load_config(), {})


class TestGetSetDelete(_ConfigDirMixin, unittest.TestCase):
    def test_get_returns_none_when_absent(self):
        self.assertIsNone(get_config_value("access_token"))

    def test_set_then_get(self):
        set_config_value("access_token", "my-token")
        self.assertEqual(get_config_value("access_token"), "my-token")

    def test_delete_removes_key(self):
        set_config_value("access_token", "my-token")
        delete_config_value("access_token")
        self.assertIsNone(get_config_value("access_token"))

    def test_delete_absent_key_is_noop(self):
        delete_config_value("nonexistent")
        self.assertEqual(load_config(), {})

    def test_file_created_on_first_write(self):
        config_path = os.path.join(self._tmpdir, "config.json")
        self.assertFalse(os.path.exists(config_path))
        set_config_value("onprem_url", "https://cs.example.com")
        self.assertTrue(os.path.exists(config_path))


class TestApplyConfigToEnv(_ConfigDirMixin, unittest.TestCase):
    def test_populates_missing_env_vars(self):
        save_config({"onprem_url": "https://cs.example.com"})
        os.environ.pop("CS_ONPREM_URL", None)

        apply_config_to_env()

        self.assertEqual(os.environ.get("CS_ONPREM_URL"), "https://cs.example.com")

    def test_does_not_overwrite_existing_env_vars(self):
        save_config({"onprem_url": "https://from-file.example.com"})
        os.environ["CS_ONPREM_URL"] = "https://from-env.example.com"

        apply_config_to_env()

        self.assertEqual(os.environ["CS_ONPREM_URL"], "https://from-env.example.com")

    def test_ignores_unknown_keys_in_file(self):
        save_config({"unknown_key": "should-be-ignored"})
        apply_config_to_env()
        self.assertNotIn("unknown_key", os.environ)


class TestGetEffectiveValue(_ConfigDirMixin, unittest.TestCase):
    def test_returns_not_set_when_absent(self):
        os.environ.pop("CS_ONPREM_URL", None)
        value, source = get_effective_value("onprem_url")
        self.assertIsNone(value)
        self.assertEqual(source, "not set")

    def test_env_takes_precedence(self):
        os.environ["CS_ONPREM_URL"] = "https://from-env.example.com"
        _snapshot_client_env_vars()
        set_config_value("onprem_url", "https://from-file.example.com")

        value, source = get_effective_value("onprem_url")
        self.assertEqual(value, "https://from-env.example.com")
        self.assertEqual(source, "environment")

    def test_falls_back_to_config_file(self):
        os.environ.pop("CS_ONPREM_URL", None)
        set_config_value("onprem_url", "https://from-file.example.com")

        value, source = get_effective_value("onprem_url")
        self.assertEqual(value, "https://from-file.example.com")
        self.assertEqual(source, "config file")

    def test_unknown_key_returns_not_set(self):
        value, source = get_effective_value("bogus")
        self.assertIsNone(value)
        self.assertEqual(source, "not set")


class TestMaskSensitiveValue(unittest.TestCase):
    def test_masks_long_value(self):
        self.assertEqual(mask_sensitive_value("secret-token-abc123"), "...abc123")

    def test_masks_short_value(self):
        self.assertEqual(mask_sensitive_value("short"), "***")

    def test_masks_exactly_tail_length(self):
        self.assertEqual(mask_sensitive_value("123456"), "***")


# --- Configure tool tests ---


class TestGetConfigTool(_ConfigDirMixin, unittest.TestCase):
    def setUp(self):
        super().setUp()
        self.tool = Configure(FastMCP("Test"), {})

    def test_get_single_key(self):
        os.environ.pop("CS_ONPREM_URL", None)
        set_config_value("onprem_url", "https://cs.example.com")

        result = json.loads(self.tool.get_config(key="onprem_url"))
        self.assertEqual(result["key"], "onprem_url")
        self.assertEqual(result["value"], "https://cs.example.com")
        self.assertEqual(result["source"], "config file")

    def test_get_unknown_key(self):
        result = json.loads(self.tool.get_config(key="bogus"))
        self.assertIn("error", result)
        self.assertIn("bogus", result["error"])
        self.assertIn("valid_keys", result)

    def test_get_all(self):
        result = json.loads(self.tool.get_config())
        self.assertIn("config_dir", result)
        self.assertIsInstance(result["options"], list)
        keys = [opt["key"] for opt in result["options"]]
        for key in ["access_token", "ace_access_token", "ca_bundle"]:
            self.assertIn(key, keys)

    @patch("configure.helpers.is_standalone_token", return_value=False)
    def test_get_all_shows_api_only_for_non_standalone(self, _mock):
        result = json.loads(self.tool.get_config())
        keys = [opt["key"] for opt in result["options"]]
        self.assertIn("onprem_url", keys)
        self.assertIn("default_project_id", keys)

    @patch("configure.helpers.is_standalone_token", return_value=True)
    def test_get_all_hides_api_only_for_standalone(self, _mock):
        result = json.loads(self.tool.get_config())
        keys = [opt["key"] for opt in result["options"]]
        self.assertNotIn("onprem_url", keys)
        self.assertNotIn("default_project_id", keys)

    def test_get_all_hides_hidden_options(self):
        result = json.loads(self.tool.get_config())
        keys = [opt["key"] for opt in result["options"]]
        self.assertNotIn("disable_tracking", keys)
        self.assertNotIn("disable_version_check", keys)

    def test_hidden_option_accessible_by_explicit_key(self):
        os.environ.pop("CS_DISABLE_TRACKING", None)
        set_config_value("disable_tracking", "true")

        result = json.loads(self.tool.get_config(key="disable_tracking"))
        self.assertEqual(result["key"], "disable_tracking")
        self.assertEqual(result["value"], "true")

    def test_sensitive_value_is_masked(self):
        os.environ["CS_ACCESS_TOKEN"] = "super-secret-token-xyz789"
        _snapshot_client_env_vars()

        result = json.loads(self.tool.get_config(key="access_token"))
        self.assertNotEqual(result["value"], "super-secret-token-xyz789")
        self.assertEqual(result["value"], "...xyz789")

    def test_get_single_includes_docs_url(self):
        os.environ.pop("CS_ONPREM_URL", None)
        set_config_value("onprem_url", "https://cs.example.com")

        result = json.loads(self.tool.get_config(key="onprem_url"))
        self.assertIn("docs_url", result)
        self.assertIn("onprem_url", result["docs_url"])

    def test_get_single_no_docs_url_for_undocumented(self):
        os.environ.pop("CS_DISABLE_TRACKING", None)
        set_config_value("disable_tracking", "true")

        result = json.loads(self.tool.get_config(key="disable_tracking"))
        self.assertNotIn("docs_url", result)


class TestSetConfigTool(_ConfigDirMixin, unittest.TestCase):
    def setUp(self):
        super().setUp()
        self.tool = Configure(FastMCP("Test"), {})

    def test_set_persists_to_file(self):
        self.tool.set_config(key="onprem_url", value="https://cs.example.com")

        config = json.loads(
            open(os.path.join(self._tmpdir, "config.json")).read()
        )
        self.assertEqual(config["onprem_url"], "https://cs.example.com")

    def test_set_updates_env(self):
        os.environ.pop("CS_ONPREM_URL", None)
        self.tool.set_config(key="onprem_url", value="https://cs.example.com")
        self.assertEqual(os.environ.get("CS_ONPREM_URL"), "https://cs.example.com")

    def test_set_returns_confirmation(self):
        result = json.loads(self.tool.set_config(key="onprem_url", value="https://cs.example.com"))
        self.assertEqual(result["status"], "saved")
        self.assertEqual(result["key"], "onprem_url")
        self.assertIn("config_dir", result)

    def test_set_unknown_key(self):
        result = json.loads(self.tool.set_config(key="bogus", value="anything"))
        self.assertIn("error", result)
        self.assertIn("bogus", result["error"])

    def test_set_empty_value_deletes(self):
        set_config_value("onprem_url", "https://cs.example.com")
        _snapshot_client_env_vars()
        os.environ["CS_ONPREM_URL"] = "https://cs.example.com"

        result = json.loads(self.tool.set_config(key="onprem_url", value=""))
        self.assertEqual(result["status"], "removed")
        self.assertIsNone(get_config_value("onprem_url"))
        self.assertNotIn("CS_ONPREM_URL", os.environ)

    def test_set_warns_about_env_override(self):
        os.environ["CS_ONPREM_URL"] = "https://from-env.example.com"
        _snapshot_client_env_vars()
        result = json.loads(self.tool.set_config(key="onprem_url", value="https://from-file.example.com"))
        self.assertIn("warning", result)
        self.assertIn("environment variable", result["warning"])
        self.assertIn("precedence", result["warning"])

    def test_set_access_token_warns_about_restart(self):
        os.environ.pop("CS_ACCESS_TOKEN", None)
        result = json.loads(self.tool.set_config(key="access_token", value="new-token"))
        self.assertIn("restart_required", result)
        self.assertIn("restart", result["restart_required"])

    def test_set_hidden_option_succeeds(self):
        result = json.loads(self.tool.set_config(key="disable_version_check", value="true"))
        self.assertEqual(result["status"], "saved")
        self.assertEqual(get_config_value("disable_version_check"), "true")

    def test_set_includes_docs_url(self):
        result = json.loads(self.tool.set_config(key="onprem_url", value="https://cs.example.com"))
        self.assertIn("docs_url", result)
        self.assertIn("onprem_url", result["docs_url"])


class TestListableOptions(_ConfigDirMixin, unittest.TestCase):
    @patch("configure.helpers.is_standalone_token", return_value=False)
    def test_non_standalone_includes_api_only(self, _mock):
        options = get_listable_options()
        self.assertIn("onprem_url", options)
        self.assertIn("default_project_id", options)

    @patch("configure.helpers.is_standalone_token", return_value=True)
    def test_standalone_excludes_api_only(self, _mock):
        options = get_listable_options()
        self.assertNotIn("onprem_url", options)
        self.assertNotIn("default_project_id", options)

    @patch("configure.helpers.is_standalone_token", return_value=False)
    def test_always_excludes_hidden(self, _mock):
        options = get_listable_options()
        self.assertNotIn("disable_tracking", options)
        self.assertNotIn("disable_version_check", options)

    @patch("configure.helpers.is_standalone_token", return_value=False)
    def test_includes_visible_options(self, _mock):
        options = get_listable_options()
        self.assertIn("access_token", options)
        self.assertIn("ace_access_token", options)
        self.assertIn("ca_bundle", options)


if __name__ == "__main__":
    unittest.main()
