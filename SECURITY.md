# Security

honrp runs as your user, with no root and no non-loopback listeners. The GUI
and daemon use loopback TCP (`127.0.0.1:47831` and `:47832`); the only outbound
request is the optional GitHub update check. It talks to Discord over its local
unix socket.

If you find a security issue (path traversal in the installer, unsafe handling
of log/config content, service-unit injection, ...), please open a **private
security advisory** on GitHub instead of a public issue, or contact the
maintainer listed in the repository profile.

Please include: distribution, prefix (redact the username), the affected
command, and the `honrp doctor` output with personal paths redacted.
