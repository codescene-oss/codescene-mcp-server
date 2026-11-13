# typed: false
# frozen_string_literal: true

class CsMcp < Formula
  desc "MCP Server exposing Code Health analysis as AI-friendly tools"
  homepage "https://github.com/codescene-oss/codehealth-mcp"
  version "1.5.21"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codescene-oss/codehealth-mcp/releases/download/MCP-#{version}/cs-mcp-macos-aarch64.zip"
      sha256 "fcaca5472fe5eb8dca8d2d9ab80e22793f2ed1db3cb17cd4a7ff3d9cfb207d92"

      define_method(:install) do
        bin.install "cs-mcp-macos-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codehealth-mcp/releases/download/MCP-#{version}/cs-mcp-macos-amd64.zip"
      sha256 "84fb4fc4800c0d5a9f4bb55cb0cb06832edf42155aacf953630301188b60309b"

      define_method(:install) do
        bin.install "cs-mcp-macos-amd64" => "cs-mcp"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/codescene-oss/codehealth-mcp/releases/download/MCP-#{version}/cs-mcp-linux-aarch64.zip"
      sha256 "cf185cf4ffdc197320ec12997e8017cfc7bc3259d2e2b23eaa04a7da42b0bc7e"

      define_method(:install) do
        bin.install "cs-mcp-linux-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codehealth-mcp/releases/download/MCP-#{version}/cs-mcp-linux-amd64.zip"
      sha256 "ab793fca7b33fad89cfb0119951d935fa3c887cd880cf688ddc57268b620705f"

      define_method(:install) do
        bin.install "cs-mcp-linux-amd64" => "cs-mcp"
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cs-mcp --version 2>&1", 2)
  end
end
