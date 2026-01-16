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


def _get_ssl_truststore_options() -> str:
    """
    Returns Java truststore options for custom CA certificates.
    
    Checks these environment variables in order of precedence:
    
    1. REQUESTS_CA_BUNDLE: Standard Python/requests CA bundle path (PEM format).
       This allows users to configure SSL certificates once for both the Python
       MCP server and the embedded Java CLI.
       
    2. SSL_CERT_FILE: Alternative standard CA certificate path (PEM format).
    
    3. CURL_CA_BUNDLE: curl-style CA bundle path (PEM format).
    
    The PEM certificate is converted to a PKCS12 truststore at runtime for Java.
    Requires the 'cryptography' package.
    
    Returns:
        Java options string for SSL configuration, or empty string if not configured.
    """
    # Check standard CA bundle environment variables in order of precedence
    ca_cert_path = (
        os.getenv("REQUESTS_CA_BUNDLE") or 
        os.getenv("SSL_CERT_FILE") or
        os.getenv("CURL_CA_BUNDLE")
    )
    
    if not ca_cert_path:
        return ""
    
    if not os.path.isfile(ca_cert_path):
        return ""
    
    truststore = _create_truststore_from_pem(ca_cert_path)
    if truststore:
        return (
            f'-Djavax.net.ssl.trustStore="{truststore}" '
            f'-Djavax.net.ssl.trustStoreType=PKCS12 '
            f'-Djavax.net.ssl.trustStorePassword=changeit'
        )
    
    return ""


def _create_truststore_from_pem(pem_path: str) -> str | None:
    """
    Creates a PKCS12 truststore from a PEM certificate file.
    
    Args:
        pem_path: Path to the PEM certificate file
        
    Returns:
        Path to the created PKCS12 truststore, or None if creation failed
    """
    import tempfile
    import hashlib
    
    try:
        from cryptography import x509
        from cryptography.hazmat.primitives.serialization import pkcs12, BestAvailableEncryption
    except ImportError:
        # cryptography not available - can't convert PEM
        return None
    
    try:
        # Read and parse the PEM certificate(s)
        with open(pem_path, 'rb') as f:
            pem_data = f.read()
        
        # Parse all certificates in the PEM file
        certs = []
        for cert in x509.load_pem_x509_certificates(pem_data):
            certs.append(pkcs12.PKCS12Certificate(cert, None))
        
        if not certs:
            return None
        
        # Create a deterministic path based on the cert content
        # so we don't recreate it on every invocation
        cert_hash = hashlib.sha256(pem_data).hexdigest()[:16]
        truststore_path = os.path.join(
            tempfile.gettempdir(), 
            f'cs-mcp-truststore-{cert_hash}.p12'
        )
        
        # Only create if it doesn't exist
        if not os.path.exists(truststore_path):
            # Use serialize_java_truststore which is designed for CA-only truststores
            p12_data = pkcs12.serialize_java_truststore(
                certs=certs,
                encryption_algorithm=BestAvailableEncryption(b"changeit")
            )
            
            with open(truststore_path, 'wb') as f:
                f.write(p12_data)
        
        return truststore_path
        
    except Exception:
        # If anything goes wrong, return None and let Java produce the SSL error
        return None


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
        """Set Java temp directory and SSL options for Windows."""
        import tempfile
        temp_dir = tempfile.gettempdir()
        options = [f'-Djava.io.tmpdir="{temp_dir}"']
        
        ssl_options = _get_ssl_truststore_options()
        if ssl_options:
            options.append(ssl_options)
        
        return ' '.join(options)


class UnixPlatformDetails(PlatformDetails):
    """Unix/Linux/macOS platform details."""
    
    def get_cli_binary_name(self) -> str:
        return "cs"
    
    def configure_environment(self, env: dict) -> dict:
        """Configure Unix-specific environment settings."""
        # Unix platforms typically don't need special PATH configuration
        return env.copy()
    
    def get_java_options(self) -> str:
        """Returns SSL truststore options if configured, empty string otherwise."""
        return _get_ssl_truststore_options()


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
