import json


def mocked_requests_post(*args, **kwargs):
    class MockResponse:
        def __init__(self, json_data, status_code):
            self.json_data = json_data
            self.status_code = status_code

        def json(self):
            return json.loads(self.json_data)
    
    # Just return a successful response for analytics tracking
    if "analytics/track" in args[0]:
        return MockResponse(json.dumps({"success": True}), 200)
    
    return MockResponse(json.dumps({}), 404)
