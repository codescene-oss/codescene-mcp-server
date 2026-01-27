import unittest
from unittest.mock import patch, MagicMock


class TestTrack(unittest.TestCase):
    
    @patch('utils.track.requests.post')
    @patch('utils.track.get_api_url', return_value='https://api.example.com')
    @patch('utils.track.get_api_request_headers', return_value={'Authorization': 'Bearer token'})
    def test_track_decorator_sends_event_on_success(self, mock_headers, mock_url, mock_post):
        from utils.track import track
        
        class MyTool:
            @track("my-event", {"key": "value"})
            def my_method(self):
                return "result"
        
        tool = MyTool()
        result = tool.my_method()
        
        self.assertEqual(result, "result")
        mock_post.assert_called_once_with(
            'https://api.example.com/v2/analytics/track',
            headers={'Authorization': 'Bearer token'},
            json={
                'event-type': 'mcp-my-event',
                'event-properties': {'key': 'value'}
            }
        )

    @patch('utils.track.requests.post')
    @patch('utils.track.get_api_url', return_value='https://api.example.com')
    @patch('utils.track.get_api_request_headers', return_value={'Authorization': 'Bearer token'})
    def test_track_decorator_with_no_properties(self, mock_headers, mock_url, mock_post):
        from utils.track import track
        
        class MyTool:
            @track("simple-event")
            def my_method(self):
                return "ok"
        
        tool = MyTool()
        tool.my_method()
        
        mock_post.assert_called_once_with(
            'https://api.example.com/v2/analytics/track',
            headers={'Authorization': 'Bearer token'},
            json={
                'event-type': 'mcp-simple-event',
                'event-properties': {}
            }
        )

    @patch('utils.track.requests.post')
    @patch('utils.track.get_api_url', return_value='https://api.example.com')
    @patch('utils.track.get_api_request_headers', return_value={'Authorization': 'Bearer token'})
    def test_track_error_sends_error_event(self, mock_headers, mock_url, mock_post):
        from utils.track import track_error
        
        error = ValueError("Something went wrong")
        track_error("my-event", error)
        
        mock_post.assert_called_once_with(
            'https://api.example.com/v2/analytics/track',
            headers={'Authorization': 'Bearer token'},
            json={
                'event-type': 'mcp-my-event-error',
                'event-properties': {'error': 'Something went wrong'}
            }
        )

    @patch('utils.track.requests.post')
    @patch('utils.track.get_api_url', return_value='https://api.example.com')
    @patch('utils.track.get_api_request_headers', return_value={'Authorization': 'Bearer token'})
    def test_track_error_appends_error_suffix_to_event_type(self, mock_headers, mock_url, mock_post):
        from utils.track import track_error
        
        track_error("select-project", Exception("API failed"))
        
        call_args = mock_post.call_args
        event_type = call_args[1]['json']['event-type']
        self.assertEqual(event_type, 'mcp-select-project-error')

    @patch('utils.track.requests.post')
    @patch('utils.track.get_api_url', return_value='https://api.example.com')
    @patch('utils.track.get_api_request_headers', return_value={'Authorization': 'Bearer token'})
    def test_track_decorator_fails_silently_on_network_error(self, mock_headers, mock_url, mock_post):
        from utils.track import track
        
        mock_post.side_effect = Exception("Network error")
        
        class MyTool:
            @track("my-event")
            def my_method(self):
                return "result"
        
        tool = MyTool()
        result = tool.my_method()
        
        # Should return result normally despite tracking failure
        self.assertEqual(result, "result")

    @patch('utils.track.requests.post')
    @patch('utils.track.get_api_url', return_value='https://api.example.com')
    @patch('utils.track.get_api_request_headers', return_value={'Authorization': 'Bearer token'})
    def test_track_error_fails_silently_on_network_error(self, mock_headers, mock_url, mock_post):
        from utils.track import track_error
        
        mock_post.side_effect = Exception("Network error")
        
        # Should not raise an exception
        track_error("my-event", ValueError("Some error"))


if __name__ == '__main__':
    unittest.main()
