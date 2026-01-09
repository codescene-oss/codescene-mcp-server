"""
Version checker for CodeScene MCP Server.
Checks if the current version is up-to-date with the latest GitHub release.
"""
import logging
import time
from functools import wraps
from typing import Optional, Callable
from dataclasses import dataclass
import requests
from version import __version__

logger = logging.getLogger(__name__)

GITHUB_REPO = "codescene-oss/codescene-mcp-server"
GITHUB_API_URL = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
CHECK_TIMEOUT = 5  # seconds
CACHE_DURATION = 3600  # Cache version check for 1 hour


@dataclass
class VersionInfo:
    """Version information result."""
    current: str
    latest: Optional[str]
    outdated: bool
    message: str


class VersionChecker:
    """Checks and caches version information."""
    
    def __init__(self, cache_duration: int = CACHE_DURATION):
        self._cache: Optional[VersionInfo] = None
        self._last_check_time: float = 0
        self._cache_duration = cache_duration

    @staticmethod
    def get_latest_version() -> Optional[str]:
        """Fetch the latest version from GitHub releases."""
        try:
            response = requests.get(
                GITHUB_API_URL,
                headers={
                    'Accept': 'application/vnd.github.v3+json',
                    'User-Agent': 'CodeScene-MCP-Server'
                },
                timeout=CHECK_TIMEOUT
            )
            response.raise_for_status()
            return response.json().get('tag_name')
        except Exception as e:
            logger.debug(f"Could not fetch latest version: {e}")
            return None

    def check_version(self) -> Optional[VersionInfo]:
        """Check if the current version is up-to-date using cached results."""
        current_time = time.time()
        
        if self._cache and (current_time - self._last_check_time) < self._cache_duration:
            return self._cache
        
        current_version = __version__
        
        if current_version == "dev":
            result = VersionInfo(
                current=current_version,
                latest=None,
                outdated=False,
                message="Running development version"
            )
            self._cache = result
            self._last_check_time = current_time
            return result
        
        latest_version = self.get_latest_version()
        
        if latest_version is None:
            logger.debug("Could not determine latest version")
            return None
        
        outdated = current_version != latest_version
        message = ""
        
        if outdated:
            message = (
                f"⚠️  CodeScene MCP Server version {current_version} is outdated. "
                f"Latest version is {latest_version}. "
                f"Please update using your installation method:\n"
                f"  • Homebrew: `brew upgrade cs-mcp`\n"
                f"  • Windows: Re-run the PowerShell install script\n"
                f"  • Docker: `docker pull codescene/codescene-mcp:latest`\n"
                f"  • Manual: Download the latest binary from "
                f"https://github.com/codescene-oss/codescene-mcp-server/releases"
            )
        
        result = VersionInfo(
            current=current_version,
            latest=latest_version,
            outdated=outdated,
            message=message
        )
        
        self._cache = result
        self._last_check_time = current_time
        
        return result


_default_checker = VersionChecker()


def check_version() -> Optional[VersionInfo]:
    """Check if the current version is up-to-date."""
    return _default_checker.check_version()


VERSION_NOTICE_DOCSTRING_SUFFIX = """
Note: If the result contains version update information (indicated by 
"VERSION UPDATE AVAILABLE"), please inform the user about this update 
notice and recommend they update their CodeScene MCP Server.
"""


def with_version_check(func: Callable) -> Callable:
    """Decorator that prepends version warning to tool output if outdated.
    
    Also appends a note to the function's docstring instructing the LLM
    to mention version updates to the user.
    """
    @wraps(func)
    def wrapper(*args, **kwargs):
        result = func(*args, **kwargs)
        version_info = check_version()
        
        if version_info and version_info.outdated:
            separator = "=" * 80
            warning = (
                f"\n{separator}\n"
                f"⚠️  VERSION UPDATE AVAILABLE:\n"
                f"{version_info.message}\n"
                f"{separator}\n\n"
            )
            
            if isinstance(result, str):
                return warning + result
            else:
                return warning + str(result)
        
        return result
    
    # Modify the docstring to include version notice instructions
    if wrapper.__doc__:
        wrapper.__doc__ = wrapper.__doc__ + VERSION_NOTICE_DOCSTRING_SUFFIX
    else:
        wrapper.__doc__ = VERSION_NOTICE_DOCSTRING_SUFFIX.strip()
    
    return wrapper
