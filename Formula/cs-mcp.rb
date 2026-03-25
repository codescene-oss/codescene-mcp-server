# typed: false
# frozen_string_literal: true

class CsMcp < Formula
  desc "MCP Server exposing Code Health analysis as AI-friendly tools"
  homepage "https://github.com/codescene-oss/codescene-mcp-server"
  version "0.3.6"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-aarch64.zip"
      sha256 "e13efb61f23f68023d0265191476fe49affa6f874f51aa319d6d51ef310f24f7"

      define_method(:install) do
        bin.install "cs-mcp-macos-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-amd64.zip"
      sha256 "b922ea7b725b5f58c4debbf0b5acaa1e369cd9f6bbfa1f6e072de21c719d7ec7"

      define_method(:install) do
        bin.install "cs-mcp-macos-amd64" => "cs-mcp"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-aarch64.zip"
      sha256 "d29dcc7dd70598e4831d1b5dc9398f7b0a22c69eae7233381f224588874a779e"

      define_method(:install) do
        bin.install "cs-mcp-linux-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-amd64.zip"
      sha256 "539902f212f134e4bddec44bb4d3f81f5b7daddeb45fc56cbb81ac65ddbc9038"

      define_method(:install) do
        bin.install "cs-mcp-linux-amd64" => "cs-mcp"
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cs-mcp --version 2>&1", 2)
  end
end
