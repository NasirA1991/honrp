# Security

honrp runs as your user, with no root and no network listeners. It talks only
to Discord's local unix socket.

If you find a security issue (path traversal in the installer, unsafe handling
of log/config content, service-unit injection, ...), please open a **private
security advisory** on GitHub instead of a public issue, or contact the
maintainer listed in the repository profile.

Please include: distribution, prefix (redact the username), the affected
command, and the `honrp doctor` output with personal paths redacted.
