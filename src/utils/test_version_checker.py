import unittest
from unittest.mock import patch, MagicMock
from utils.version_checker import (
    VersionChecker,
    with_version_check
)


class TestGetLatestVersion(unittest.TestCase):
    
    @patch('requests.get')
    def test_successful_fetch(self, mock_get):
        mock_response = MagicMock()
        mock_response.json.return_value = {'tag_name': 'MCP-1.2.3'}
        mock_get.return_value = mock_response
        
        version = VersionChecker.get_latest_version()
        self.assertEqual(version, 'MCP-1.2.3')
    
    @patch('requests.get')
    def test_failed_fetch(self, mock_get):
        mock_get.side_effect = Exception("Network error")
        
        version = VersionChecker.get_latest_version()
        self.assertIsNone(version)


class TestVersionChecker(unittest.TestCase):
    
    def setUp(self):
        self.checker = VersionChecker(cache_duration=3600)
    
    @patch('utils.version_checker.__version__', 'dev')
    def test_dev_version_check(self):
        result = self.checker.check_version()
        
        self.assertIsNotNone(result)
        self.assertEqual(result.current, 'dev')
        self.assertFalse(result.outdated)
        self.assertIn('development', result.message.lower())
    
    @patch('utils.version_checker.__version__', 'MCP-1.0.0')
    @patch.object(VersionChecker, 'get_latest_version')
    def test_outdated_version_check(self, mock_get_latest):
        mock_get_latest.return_value = 'MCP-2.0.0'
        
        result = self.checker.check_version()
        
        self.assertIsNotNone(result)
        self.assertEqual(result.current, 'MCP-1.0.0')
        self.assertEqual(result.latest, 'MCP-2.0.0')
        self.assertTrue(result.outdated)
        self.assertIn('outdated', result.message.lower())
        self.assertIn('docker pull', result.message)
    
    @patch('utils.version_checker.__version__', 'MCP-2.0.0')
    @patch.object(VersionChecker, 'get_latest_version')
    def test_current_version_check(self, mock_get_latest):
        mock_get_latest.return_value = 'MCP-2.0.0'
        
        result = self.checker.check_version()
        
        self.assertIsNotNone(result)
        self.assertEqual(result.current, 'MCP-2.0.0')
        self.assertEqual(result.latest, 'MCP-2.0.0')
        self.assertFalse(result.outdated)
        self.assertEqual(result.message, '')
    
    @patch('utils.version_checker.__version__', 'MCP-1.0.0')
    @patch.object(VersionChecker, 'get_latest_version')
    def test_failed_version_check(self, mock_get_latest):
        mock_get_latest.return_value = None
        
        result = self.checker.check_version()
        
        self.assertIsNone(result)
    
    @patch('utils.version_checker.__version__', 'MCP-1.0.0')
    @patch.object(VersionChecker, 'get_latest_version')
    def test_version_check_caching(self, mock_get_latest):
        mock_get_latest.return_value = 'MCP-2.0.0'
        
        result1 = self.checker.check_version()
        self.assertEqual(mock_get_latest.call_count, 1)
        
        result2 = self.checker.check_version()
        self.assertEqual(mock_get_latest.call_count, 1)
        self.assertEqual(result1, result2)


class TestWithVersionCheck(unittest.TestCase):
    
    @patch('utils.version_checker.check_version')
    def test_decorator_with_current_version(self, mock_check):
        from utils.version_checker import VersionInfo
        mock_check.return_value = VersionInfo(
            current='MCP-1.0.0',
            latest='MCP-1.0.0',
            outdated=False,
            message='Up to date'
        )
        
        @with_version_check
        def sample_tool():
            return "Tool result"
        
        result = sample_tool()
        self.assertEqual(result, "Tool result")
    
    @patch('utils.version_checker.check_version')
    def test_decorator_with_outdated_version(self, mock_check):
        from utils.version_checker import VersionInfo
        mock_check.return_value = VersionInfo(
            current='MCP-1.0.0',
            latest='MCP-2.0.0',
            outdated=True,
            message='Version outdated'
        )
        
        @with_version_check
        def sample_tool():
            return "Tool result"
        
        result = sample_tool()
        self.assertIn('Version outdated', result)
        self.assertIn('Tool result', result)
        self.assertIn('VERSION UPDATE AVAILABLE', result)
        self.assertIn('=' * 80, result)
    
    @patch('utils.version_checker.check_version')
    def test_decorator_with_failed_check(self, mock_check):
        mock_check.return_value = None
        
        @with_version_check
        def sample_tool():
            return "Tool result"
        
        result = sample_tool()
        self.assertEqual(result, "Tool result")


if __name__ == '__main__':
    unittest.main()
