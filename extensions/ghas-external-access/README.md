# GHAS External Access Configuration

This provides external access to the GHAS (GitHub Advanced Security) MCP server for Zed agent functionality.

## What This Provides

- External access to GHAS MCP server from mobile devices
- Simple configuration for remote access
- Integration with existing Zed agent infrastructure

## Configuration

### 1. Basic Setup

Add this to your Zed settings (`~/.config/zed/settings.json`):

```json
{
  "context_servers": {
    "ghas-external": {
      "command": "uvx",
      "args": [
        "mcp-remote",
        "http://your-external-ip:25113/mcp"
      ],
      "enabled": true
    }
  }
}
```

### 2. For Tailscale Access

If using Tailscale for secure remote access:

```json
{
  "context_servers": {
    "ghas-tailscale": {
      "command": "uvx",
      "args": [
        "mcp-remote",
        "https://your-tailscale-host.tailnet.ts.net:25113/mcp"
      ],
      "enabled": true
    }
  }
}
```

### 3. Direct Local Access

For local development/testing:

```json
{
  "context_servers": {
    "ghas-local": {
      "command": "uvx",
      "args": [
        "mcp-remote",
        "http://127.0.0.1:25113/mcp"
      ],
      "enabled": true
    }
  }
}
```

## Requirements

- GHAS MCP server must be running (`ghas-mcp-stdio.sh`)
- Port 25113 must be accessible (or configured port)
- For external access: proper firewall/NAT configuration
- For secure access: Tailscale or similar VPN recommended

## Security Notes

1. **Do not expose MCP servers directly to the internet**
2. Use Tailscale, WireGuard, or other VPN solutions
3. Consider adding authentication to your MCP proxy
4. Monitor access logs regularly

## Usage

Once configured, the GHAS MCP server will be available in Zed's agent panel for:
- GitHub code search
- Repository queries
- Advanced security analysis
- Code navigation

## Troubleshooting

- Verify GHAS MCP is running: `pgrep -f ghas-mcp-stdio.sh`
- Check port accessibility: `nc -zv 127.0.0.1 25113`
- Review logs: `journalctl -u mcpproxy -f`
- Test MCP connection: `curl http://localhost:25113/mcp/tools`

## Advanced Configuration

For production use, consider:
- Adding authentication middleware
- Setting up rate limiting
- Configuring TLS termination
- Implementing IP whitelisting
