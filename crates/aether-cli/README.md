# Aether CLI

## `aether logs`

Stream application logs from the control-plane.

Flags:
- --app <name> (default: $AETHER_DEFAULT_APP or "sample-app")
- --follow (keep connection and auto-reconnect)
- --since <duration|RFC3339>
- --container <name>
- --format json|text (default: text)
- --color (optional colorization)

Environment overrides:
- AETHER_API_BASE: control-plane base URL (e.g., http://localhost:8080)
- AETHER_LOGS_FOLLOW=1: default follow behavior
- AETHER_LOGS_FORMAT=json|text: default format
- AETHER_LOGS_CONTAINER: default container
- AETHER_LOGS_SINCE: default since filter
- AETHER_LOGS_TAIL: tail lines (default 100)
- AETHER_LOGS_MAX_RECONNECTS: cap reconnect attempts
- AETHER_LOGS_MOCK=1: mock output without network (used in tests/CI)

Examples:
- aether logs --app demo --follow
- AETHER_LOGS_FORMAT=json aether logs --app demo
- AETHER_LOGS_MOCK=1 aether logs --app demo --format text
