import unittest
from unittest import mock
import sys
from .platform_details import (
    get_platform_details,
    WindowsPlatformDetails,
    UnixPlatformDetails
)


class TestPlatformDetails(unittest.TestCase):
    
    def test_get_platform_details_returns_details(self):
        # Just test that it returns a details instance, not the specific type
        # since we're running on a real platform
        details = get_platform_details()
        self.assertIsNotNone(details)
        # Check it has the required methods
        self.assertTrue(hasattr(details, 'get_cli_binary_name'))
        self.assertTrue(hasattr(details, 'configure_environment'))
    
    def test_windows_cli_binary_name(self):
        details = WindowsPlatformDetails()
        self.assertEqual("cs.exe", details.get_cli_binary_name())
    
    def test_unix_cli_binary_name(self):
        details = UnixPlatformDetails()
        self.assertEqual("cs", details.get_cli_binary_name())
    
    @mock.patch('os.path.exists')
    def test_windows_configure_environment_adds_git_to_path(self, mock_exists):
        mock_exists.return_value = True
        details = WindowsPlatformDetails()
        env = {'PATH': 'C:\\existing\\path'}
        
        result = details.configure_environment(env)
        
        self.assertIn('Git', result['PATH'])
        self.assertIn('C:\\existing\\path', result['PATH'])
        self.assertIn(';', result['PATH'])
    
    def test_unix_configure_environment_returns_copy(self):
        details = UnixPlatformDetails()
        env = {'PATH': '/usr/bin:/usr/local/bin'}
        
        result = details.configure_environment(env)
        
        self.assertEqual(env['PATH'], result['PATH'])
        self.assertIsNot(env, result)  # Should be a copy
    
    @mock.patch('os.path.exists')
    def test_windows_configure_environment_no_git_found(self, mock_exists):
        mock_exists.return_value = False
        details = WindowsPlatformDetails()
        env = {'PATH': 'C:\\existing\\path'}
        
        result = details.configure_environment(env)
        
        # PATH should remain unchanged if no Git found
        self.assertEqual('C:\\existing\\path', result['PATH'])
    
    @mock.patch('os.path.exists')
    def test_windows_configure_environment_git_already_in_path(self, mock_exists):
        # Return False for all paths so nothing gets added
        mock_exists.return_value = False
        details = WindowsPlatformDetails()
        git_path = r'C:\Program Files\Git\cmd'
        env = {'PATH': f'{git_path};C:\\existing\\path'}
        
        result = details.configure_environment(env)
        
        # If git is already in path and we can't find other git paths,
        # the PATH should remain unchanged
        self.assertEqual(env['PATH'], result['PATH'])
    
    def test_windows_configure_environment_preserves_other_env_vars(self):
        details = WindowsPlatformDetails()
        env = {
            'PATH': 'C:\\test',
            'HOME': 'C:\\Users\\test',
            'CUSTOM_VAR': 'value'
        }
        
        result = details.configure_environment(env)
        
        self.assertEqual('C:\\Users\\test', result['HOME'])
        self.assertEqual('value', result['CUSTOM_VAR'])
    
    def test_unix_configure_environment_preserves_all_vars(self):
        details = UnixPlatformDetails()
        env = {
            'PATH': '/usr/bin',
            'HOME': '/home/user',
            'SHELL': '/bin/bash'
        }
        
        result = details.configure_environment(env)
        
        self.assertEqual(env['PATH'], result['PATH'])
        self.assertEqual(env['HOME'], result['HOME'])
        self.assertEqual(env['SHELL'], result['SHELL'])
    
    @mock.patch('os.path.exists')
    def test_windows_finds_first_existing_git_path(self, mock_exists):
        # Simulate only the third path existing
        def exists_side_effect(path):
            return r'C:\Program Files\Git\bin' in path
        
        mock_exists.side_effect = exists_side_effect
        details = WindowsPlatformDetails()
        env = {'PATH': 'C:\\existing'}
        
        result = details.configure_environment(env)
        
        self.assertIn(r'Git\bin', result['PATH'])
        self.assertIn('C:\\existing', result['PATH'])
    
    def test_windows_get_java_options_returns_tmpdir_setting(self):
        details = WindowsPlatformDetails()
        
        result = details.get_java_options()
        
        self.assertIn('-Djava.io.tmpdir=', result)
        # Should contain a valid temp directory path
        self.assertTrue(len(result) > len('-Djava.io.tmpdir=""'))
    
    def test_unix_get_java_options_returns_empty_string(self):
        details = UnixPlatformDetails()
        
        result = details.get_java_options()
        
        self.assertEqual("", result)
    
    def _test_platform_detection(self, platform_name: str, expected_class: str):
        """Helper to test platform detection with mocked sys.platform."""
        import utils.platform_details as pd
        
        try:
            pd.sys = mock.MagicMock()
            pd.sys.platform = platform_name
            
            details = pd.get_platform_details()
            self.assertEqual(expected_class, details.__class__.__name__)
        finally:
            pd.sys = sys
    
    def test_get_platform_details_returns_windows_on_win32(self):
        self._test_platform_detection('win32', 'WindowsPlatformDetails')
    
    def test_get_platform_details_returns_unix_on_darwin(self):
        self._test_platform_detection('darwin', 'UnixPlatformDetails')
    
    def test_get_platform_details_returns_unix_on_linux(self):
        self._test_platform_detection('linux', 'UnixPlatformDetails')


if __name__ == '__main__':
    unittest.main()
