# typed: false
# frozen_string_literal: true

class CsMcp < Formula
  desc "MCP Server exposing Code Health analysis as AI-friendly tools"
  homepage "https://github.com/codescene-oss/codescene-mcp-server"
  version "1.4.5"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-aarch64.zip"
      sha256 "273c8176603fb6a3a04cb7f647d1cdd041e9d499a02d6001733bd63dd579de12"

      define_method(:install) do
        bin.install "cs-mcp-macos-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-amd64.zip"
      sha256 "35fce0d3f21097b20826e6e928bc7d960aeedb3c69885d36287d4c7c94d230ca"

      define_method(:install) do
        bin.install "cs-mcp-macos-amd64" => "cs-mcp"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-aarch64.zip"
      sha256 "706d1b1fc39a2cf9f81e2a97559a36a03254e508fa3b6ca94dbc3d8e8f7302aa"

      define_method(:install) do
        bin.install "cs-mcp-linux-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-amd64.zip"
      sha256 "7b2f3e1683a0e916fe9245824c3a1a279b5b5831e3101f259503eeb32e0ed43d"

      define_method(:install) do
        bin.install "cs-mcp-linux-amd64" => "cs-mcp"
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cs-mcp --version 2>&1", 2)
  end
end
