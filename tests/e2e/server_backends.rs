//! Server backend abstractions for e2e tests.
//!
//! Provides `CargoBackend`, `DockerBackend`, and `NpmBackend` for running
//! the MCP server in different configurations. Backend selection is driven
//! by the `CS_MCP_BACKEND` environment variable:
//!
//! - `static` (default): run the Cargo-built binary directly
//! - `docker`: run inside a Docker container
//! - `npm`: test the full npm package install + binary download flow

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};
use std::thread;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstract backend for running the MCP server.
pub trait ServerBackend {
    /// Prepare the backend (build executable, build image, etc.).
    fn prepare(&mut self) {}

    /// Get the command to launch the MCP server.
    fn get_command(&self, working_dir: &Path) -> Vec<String>;

    /// Get environment variables for the server process.
    fn get_env(
        &self,
        base_env: &HashMap<String, String>,
        working_dir: &Path,
    ) -> HashMap<String, String>;
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create and prepare the backend indicated by `CS_MCP_BACKEND` (default: `static`).
pub fn create_backend(executable: PathBuf) -> Box<dyn ServerBackend> {
    let kind = env::var("CS_MCP_BACKEND").unwrap_or_else(|_| "static".to_string());
    let mut backend: Box<dyn ServerBackend> = match kind.as_str() {
        "static" => Box::new(CargoBackend::with_executable(executable)),
        "docker" => Box::new(DockerBackend::new(None)),
        "npm" => Box::new(NpmBackend::new(Some(executable))),
        other => panic!("Unknown CS_MCP_BACKEND value: {other}"),
    };
    backend.prepare();
    backend
}

// ---------------------------------------------------------------------------
// Cargo (static) backend
// ---------------------------------------------------------------------------

/// Backend that uses a pre-built Cargo executable.
pub struct CargoBackend {
    executable: PathBuf,
}

impl CargoBackend {
    pub fn with_executable(path: PathBuf) -> Self {
        Self { executable: path }
    }
}

impl ServerBackend for CargoBackend {
    fn get_command(&self, _working_dir: &Path) -> Vec<String> {
        vec![self.executable.to_string_lossy().to_string()]
    }

    fn get_env(
        &self,
        base_env: &HashMap<String, String>,
        _working_dir: &Path,
    ) -> HashMap<String, String> {
        let mut env = base_env.clone();
        env.remove("CS_MOUNT_PATH");
        env.entry("CS_DISABLE_VERSION_CHECK".to_string())
            .or_insert_with(|| "1".to_string());
        env
    }
}

// ---------------------------------------------------------------------------
// Docker backend
// ---------------------------------------------------------------------------

/// Backend that runs the MCP server inside a Docker container.
///
/// Follows the documented pattern: `CS_MOUNT_PATH` is set to the host path,
/// the host directory is bind-mounted to `/mount/` inside the container,
/// and the server translates paths internally.
pub struct DockerBackend {
    image_name: String,
}

impl DockerBackend {
    const DEFAULT_IMAGE: &str = "codescene-mcp-test";
    const CONTAINER_MOUNT_DEST: &str = "/mount/";
    const TEST_VERSION: &str = "MCP-0.0.0-test";

    pub fn new(image_name: Option<String>) -> Self {
        Self {
            image_name: image_name.unwrap_or_else(|| Self::DEFAULT_IMAGE.to_string()),
        }
    }

    fn build_image(&self) {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let status = Command::new("docker")
            .args([
                "build",
                "--build-arg",
                &format!("VERSION={}", Self::TEST_VERSION),
                "-t",
                &self.image_name,
                ".",
            ])
            .current_dir(repo_root)
            .status()
            .expect("docker build should execute");

        assert!(status.success(), "docker build failed");
    }

    /// Docker env vars are passed via `-e` flags in `get_command`, so
    /// `get_env` only provides the host-side env for the test harness
    /// (the vars that `docker run -e NAME` will look up from the host).
    const PASSTHROUGH_VARS: &[&str] = &[
        "CS_ACCESS_TOKEN",
        "CS_ONPREM_URL",
        "CS_VERSION_CHECK_URL",
        "CS_DISABLE_VERSION_CHECK",
        "CS_TRACKING_URL",
        "CS_DISABLE_TRACKING",
        "CS_ENVIRONMENT",
        "CS_CONFIG_DIR",
        "CS_ENABLED_TOOLS",
        "CS_LOG_RETENTION_DAYS",
    ];
}

impl ServerBackend for DockerBackend {
    fn prepare(&mut self) {
        self.build_image();
    }

    fn get_command(&self, working_dir: &Path) -> Vec<String> {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let mut cmd = vec![
            "docker".to_string(),
            "run".to_string(),
            "-i".to_string(),
            "--rm".to_string(),
            "--user".to_string(),
            format!("{uid}:{gid}"),
        ];

        for var in Self::PASSTHROUGH_VARS {
            cmd.push("-e".to_string());
            cmd.push((*var).to_string());
        }

        let mount_path = working_dir.to_string_lossy();
        cmd.push("-e".to_string());
        cmd.push(format!("CS_MOUNT_PATH={mount_path}"));

        cmd.push("--add-host=host.docker.internal:host-gateway".to_string());
        cmd.push("--mount".to_string());
        cmd.push(format!(
            "type=bind,src={mount_path},dst={}",
            Self::CONTAINER_MOUNT_DEST
        ));

        cmd.push(self.image_name.clone());
        cmd
    }

    fn get_env(
        &self,
        base_env: &HashMap<String, String>,
        _working_dir: &Path,
    ) -> HashMap<String, String> {
        let mut env = base_env.clone();
        env.entry("CS_DISABLE_VERSION_CHECK".to_string())
            .or_insert_with(|| "1".to_string());
        env
    }
}

// ---------------------------------------------------------------------------
// npm backend
// ---------------------------------------------------------------------------

/// Shared state for the npm backend, prepared once via `LazyLock`.
struct NpmSharedState {
    install_dir: PathBuf,
    http_port: u16,
    _serve_dir: PathBuf,
}

/// Global npm backend state, initialized once on first access.
static NPM_STATE: LazyLock<Mutex<Option<NpmSharedState>>> =
    LazyLock::new(|| Mutex::new(None));

/// Backend that tests the full npm package install and binary download flow.
///
/// 1. Uses the Cargo-built binary
/// 2. Packs the npm package with `npm pack`
/// 3. Installs the tarball into a temp directory
/// 4. Starts a local HTTP server serving the binary as a zip
/// 5. Runs via the `node_modules/.bin/cs-mcp` symlink
pub struct NpmBackend {
    cargo_executable: Option<PathBuf>,
}

impl NpmBackend {
    pub fn new(executable: Option<PathBuf>) -> Self {
        Self {
            cargo_executable: executable,
        }
    }

    fn repo_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
    }

    fn find_tool(name: &str) -> PathBuf {
        which(name).unwrap_or_else(|| panic!("{name} not found in PATH"))
    }

    fn read_package_version() -> String {
        let pkg = Self::repo_root().join("npm/package.json");
        let content = std::fs::read_to_string(&pkg).expect("read npm/package.json");
        let v: serde_json::Value = serde_json::from_str(&content).expect("parse package.json");
        v["version"].as_str().expect("version field").to_string()
    }

    fn platform_asset_info() -> (String, String) {
        let (os_label, is_zipped) = if cfg!(target_os = "macos") {
            ("macos", true)
        } else if cfg!(target_os = "linux") {
            ("linux", true)
        } else if cfg!(target_os = "windows") {
            ("windows", false)
        } else {
            panic!("Unsupported platform for npm backend")
        };

        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "amd64"
        };

        let binary_name = format!("cs-mcp-{os_label}-{arch}");
        if is_zipped {
            (format!("{binary_name}.zip"), binary_name)
        } else {
            (format!("{binary_name}.exe"), format!("{binary_name}.exe"))
        }
    }

    fn pack_npm_package(&self) -> PathBuf {
        let npm = Self::find_tool("npm");
        let npm_dir = Self::repo_root().join("npm");
        let pack_dir = tempfile::tempdir().expect("create pack dir");
        let pack_dest = pack_dir.path().to_path_buf();

        let output = Command::new(npm)
            .args(["pack", "--pack-destination", pack_dest.to_str().unwrap()])
            .current_dir(&npm_dir)
            .output()
            .expect("npm pack should execute");

        assert!(output.status.success(), "npm pack failed: {}", String::from_utf8_lossy(&output.stderr));

        let tarball_name = String::from_utf8_lossy(&output.stdout)
            .trim()
            .lines()
            .last()
            .expect("npm pack output")
            .to_string();

        let path = pack_dest.join(tarball_name);
        assert!(path.exists(), "Expected tarball not found: {}", path.display());
        // Keep the temp dir alive by leaking it; tarball is deleted after install
        std::mem::forget(pack_dir);
        path
    }

    fn install_tarball(&self, tarball: &Path) -> PathBuf {
        let npm = Self::find_tool("npm");
        let dir = tempfile::tempdir().expect("create npm install dir");
        let install_dir = dir.keep();

        let init_pkg = r#"{"name":"npm-backend-test","version":"0.0.0","private":true}"#;
        std::fs::write(install_dir.join("package.json"), init_pkg).expect("write init pkg");

        let status = Command::new(npm)
            .args(["install", tarball.to_str().unwrap()])
            .current_dir(&install_dir)
            .status()
            .expect("npm install should execute");

        assert!(status.success(), "npm install failed");
        install_dir
    }

    fn prepare_serve_directory(&self, binary_path: &Path) -> PathBuf {
        let version = Self::read_package_version();
        let tag = format!("MCP-{version}");
        let (asset_name, inner_binary_name) = Self::platform_asset_info();

        let dir = tempfile::tempdir().expect("create serve dir");
        let serve_dir = dir.keep();
        let tag_dir = serve_dir.join(&tag);
        std::fs::create_dir_all(&tag_dir).expect("create tag dir");

        if asset_name.ends_with(".zip") {
            create_zip(&tag_dir.join(&asset_name), binary_path, &inner_binary_name);
        } else {
            std::fs::copy(binary_path, tag_dir.join(&asset_name)).expect("copy binary");
        }

        serve_dir
    }

    /// Run the bin script once to download and cache the binary.
    /// This prevents parallel tests from racing on the same cache file.
    fn prime_binary_cache(bin_script: &Path, http_port: u16) {
        let mut env: HashMap<String, String> = std::env::vars().collect();
        env.insert(
            "CS_MCP_DOWNLOAD_BASE_URL".to_string(),
            format!("http://127.0.0.1:{http_port}"),
        );
        env.remove("CS_MCP_BINARY_PATH");

        let mut child = Command::new(bin_script)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env_clear()
            .envs(&env)
            .spawn()
            .expect("prime binary cache");

        // Wait for the process to be alive (binary downloaded + started)
        std::thread::sleep(std::time::Duration::from_secs(15));

        // Kill it — we only needed the download side-effect
        let _ = child.kill();
        let _ = child.wait();
    }

    fn start_file_server(serve_dir: &Path) -> u16 {
        let server = tiny_http::Server::http("127.0.0.1:0").expect("start file server");
        let port = server.server_addr().to_ip().unwrap().port();
        let root = serve_dir.to_path_buf();

        thread::spawn(move || {
            for request in server.incoming_requests() {
                let decoded = percent_decode(request.url());
                let relative = decoded.trim_start_matches('/');
                let file_path = root.join(relative);

                if let Ok(file) = std::fs::File::open(&file_path) {
                    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                    let response = tiny_http::Response::from_file(file).with_header(
                        tiny_http::Header::from_bytes(
                            b"Content-Length" as &[u8],
                            size.to_string().as_bytes(),
                        )
                        .unwrap(),
                    );
                    let _ = request.respond(response);
                } else {
                    let response = tiny_http::Response::empty(404);
                    let _ = request.respond(response);
                }
            }
        });

        port
    }
}

impl ServerBackend for NpmBackend {
    fn prepare(&mut self) {
        let mut state = NPM_STATE.lock().unwrap();
        if state.is_some() {
            return; // Already prepared by another test
        }

        let binary = self.cargo_executable.as_ref().expect("executable required for npm backend");
        assert!(binary.exists(), "Binary not found: {}", binary.display());

        Self::find_tool("node");

        let tarball = self.pack_npm_package();
        let install_dir = self.install_tarball(&tarball);
        let _ = std::fs::remove_file(&tarball);

        let serve_dir = self.prepare_serve_directory(binary);
        let port = Self::start_file_server(&serve_dir);

        // Verify bin symlink exists
        let bin_script = install_dir.join("node_modules/.bin/cs-mcp");
        assert!(bin_script.exists(), "Bin symlink not found: {}", bin_script.display());

        // Prime the binary cache: run the bin script with --version so the
        // download + extract happens exactly once before parallel tests start.
        Self::prime_binary_cache(&bin_script, port);

        *state = Some(NpmSharedState {
            install_dir,
            http_port: port,
            _serve_dir: serve_dir,
        });
    }

    fn get_command(&self, _working_dir: &Path) -> Vec<String> {
        let state = NPM_STATE.lock().unwrap();
        let s = state.as_ref().expect("prepare() must be called first");
        let bin_script = s.install_dir.join("node_modules/.bin/cs-mcp");
        vec![bin_script.to_string_lossy().to_string()]
    }

    fn get_env(
        &self,
        base_env: &HashMap<String, String>,
        _working_dir: &Path,
    ) -> HashMap<String, String> {
        let state = NPM_STATE.lock().unwrap();
        let s = state.as_ref().expect("prepare() must be called first");
        let mut env = base_env.clone();
        env.remove("CS_MOUNT_PATH");
        env.remove("CS_MCP_BINARY_PATH");
        env.insert(
            "CS_MCP_DOWNLOAD_BASE_URL".to_string(),
            format!("http://127.0.0.1:{}", s.http_port),
        );
        env.entry("CS_DISABLE_VERSION_CHECK".to_string())
            .or_insert_with(|| "1".to_string());
        env
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build the base environment from the current process env.
pub fn base_env() -> HashMap<String, String> {
    env::vars().collect()
}

/// Find an executable on PATH (simplified `which`).
fn which(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|dir| PathBuf::from(dir).join(name))
        .find(|p| p.is_file())
}

/// Decode percent-encoded URL characters (e.g. `%20` → space).
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &input[i + 1..i + 3],
                16,
            ) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Create a zip file containing `source` as `inner_name`.
fn create_zip(zip_path: &Path, source: &Path, inner_name: &str) {
    let file = std::fs::File::create(zip_path).expect("create zip file");
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(inner_name, options).expect("start zip entry");
    let data = std::fs::read(source).expect("read binary for zip");
    std::io::Write::write_all(&mut zip, &data).expect("write zip entry");
    zip.finish().expect("finish zip");
}

/// True when running with `CS_MCP_BACKEND=docker`.
pub fn is_docker() -> bool {
    env::var("CS_MCP_BACKEND").unwrap_or_default() == "docker"
}

/// The address to bind fake HTTP servers on.
/// Use `0.0.0.0` for Docker so the container can connect back to the host.
pub fn fake_server_bind_host() -> &'static str {
    if is_docker() { "0.0.0.0" } else { "127.0.0.1" }
}

/// The hostname the MCP server should use to reach fake servers on the host.
pub fn fake_server_url_host() -> &'static str {
    if is_docker() { "host.docker.internal" } else { "127.0.0.1" }
}

/// Translate a host-side config directory path for the Docker container.
/// The directory must be inside `repo_dir` so it's accessible via the bind mount.
pub fn docker_config_dir(config_dir: &Path, repo_dir: &Path) -> String {
    if is_docker() {
        let relative = config_dir
            .strip_prefix(repo_dir)
            .expect("config_dir must be inside repo_dir for Docker");
        format!("/mount/{}", relative.display())
    } else {
        config_dir.to_string_lossy().to_string()
    }
}

/// Skip the current test when running under Docker.
/// Call at the top of tests that are not applicable for Docker.
pub fn skip_if_docker(reason: &str) {
    if is_docker() {
        eprintln!("  SKIP (Docker): {reason}");
    }
}
