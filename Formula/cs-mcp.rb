# typed: false
# frozen_string_literal: true

class CsMcp < Formula
  desc "MCP Server exposing Code Health analysis as AI-friendly tools"
  homepage "https://github.com/codescene-oss/codescene-mcp-server"
  version "1.1.6"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-aarch64.zip"
      sha256 "7f1d1b8385a8df419667ba360c9f9aff352cf2eb3b6e2c774a471524244c021e"

      define_method(:install) do
        bin.install "cs-mcp-macos-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-macos-amd64.zip"
      sha256 "c000a98d1bc332dc6eb8890ac64ebf22c4036ddde8ed343160392cf3bf9f2d44"

      define_method(:install) do
        bin.install "cs-mcp-macos-amd64" => "cs-mcp"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-aarch64.zip"
      sha256 "44afced03537da498d06f685a1000ac7d44dfd900a77cfac875246a70146605c"

      define_method(:install) do
        bin.install "cs-mcp-linux-aarch64" => "cs-mcp"
      end
    end

    on_intel do
      url "https://github.com/codescene-oss/codescene-mcp-server/releases/download/MCP-#{version}/cs-mcp-linux-amd64.zip"
      sha256 "84e81d3e6ba04460c741099ae313d395a43fd82f946e84ec67ce185915f60285"

      define_method(:install) do
        bin.install "cs-mcp-linux-amd64" => "cs-mcp"
      end
    end
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/cs-mcp --version 2>&1", 2)
  end
end
