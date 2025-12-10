import functools
import requests
from utils import get_api_url, get_api_request_headers


def track(event_type: str):
	def wrapper(f):
		@functools.wraps(f)
		def wrapped(self, *f_args, **f_kwargs):
			payload = {
				"event-type": f"mcp-{event_type}", 
				"event-properties": {}
			}

			requests.post(f"{get_api_url()}/v2/analytics/track", headers=get_api_request_headers(), json=payload)

			return f(self, *f_args, **f_kwargs)

		return wrapped

	return wrapper
