# Getting a Personal Access Token (PAT)

A Personal Access Token (PAT) is required to authenticate with the CodeScene MCP Server. This token grants access to the Code Health analysis capability.

## CodeScene Cloud

If you're using CodeScene Cloud, create your token here:

👉 **[Create a Personal Access Token](https://codescene.io/users/me/pat)**

## CodeScene On-Prem

If you're using CodeScene on-prem, follow these steps to create a Personal Access Token:

1. **Log in to your CodeScene instance**  
   Contact your CodeScene admin if you do not know the URL.

2. **Navigate to the Configuration menu**  
   Click on the Configuration menu in the top navigation.

3. **Go to the Authentication tab**  
   Select the Authentication tab from the configuration options.

4. **Create a new Personal Access Token**  
   Click **Personal Access Tokens** under the Authentication & User Management section to create a new token.

Alternatively, you can navigate directly to:

```
https://<your-cs-host><:port>/configuration/user/token
```

## Using Your Token

Once you have your token, set it as the `CS_ACCESS_TOKEN` environment variable when configuring the MCP server. See the installation guides for your platform:

- [Homebrew Installation (macOS / Linux)](homebrew-installation.md)
- [Windows Installation](windows-installation.md)
- [Docker Installation](docker-installation.md)

> ⚠️ **Keep your token secure!** Treat it like a password and never commit it to version control.
