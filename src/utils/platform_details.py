import os
import sys
from abc import ABC, abstractmethod


class PlatformDetails(ABC):
    """Abstract base class for platform-specific operations."""
    
    @abstractmethod
    def get_cli_binary_name(self) -> str:
        """Returns the platform-specific CLI binary name."""
        pass
    
    
    @abstractmethod
    def configure_environment(self, env: dict) -> dict:
        """
        Configure platform-specific environment variables.
        
        Args:
            env: Base environment dictionary
            
        Returns:
            Modified environment dictionary
        """
        pass
    
    @abstractmethod
    def get_java_options(self) -> str:
        """Returns platform-specific Java options."""
        pass


class WindowsPlatformDetails(PlatformDetails):
    """Windows-specific platform details."""
    
    def get_cli_binary_name(self) -> str:
        return "cs.exe"
    
    def configure_environment(self, env: dict) -> dict:
        """Configure Windows-specific environment settings."""
        env = env.copy()
        
        # Ensure Git is findable in PATH for bundled executables
        common_git_paths = [
            r"C:\Program Files\Git\cmd",
            r"C:\Program Files (x86)\Git\cmd",
            r"C:\Program Files\Git\bin",
            r"C:\Program Files (x86)\Git\bin",
        ]
        
        existing_path = env.get('PATH', '')
        for git_path in common_git_paths:
            if os.path.exists(git_path) and git_path not in existing_path:
                env['PATH'] = f"{git_path};{existing_path}"
                break
        
        return env
    
    def get_java_options(self) -> str:
        """Set Java temp directory to avoid Windows directory access issues."""
        import tempfile
        temp_dir = tempfile.gettempdir()
        return f'-Djava.io.tmpdir="{temp_dir}"'


class UnixPlatformDetails(PlatformDetails):
    """Unix/Linux/macOS platform details."""
    
    def get_cli_binary_name(self) -> str:
        return "cs"
    
    def configure_environment(self, env: dict) -> dict:
        """Configure Unix-specific environment settings."""
        # Unix platforms typically don't need special PATH configuration
        return env.copy()
    
    def get_java_options(self) -> str:
        """Unix platforms typically don't need special Java options."""
        return ""


def get_platform_details() -> PlatformDetails:
    """
    Factory function to get the appropriate platform details.
    
    Returns:
        PlatformDetails instance for the current platform
    """
    if sys.platform == "win32":
        return WindowsPlatformDetails()
    else:
        # Unix-like systems (Linux, macOS, etc.)
        return UnixPlatformDetails()
