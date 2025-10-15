# Control Plane Auth & RBAC

Env configuration:
- AETHER_API_TOKENS: CSV entries in form token:role[:name], roles: admin, reader
- AETHER_AUTH_REQUIRED: 1 to enforce auth, 0/absent to disable (default disabled for backward-compat)

Example:
- export AETHER_API_TOKENS="t_admin:admin:alice,t_reader:reader:bob"
- export AETHER_AUTH_REQUIRED=1

Requests:
- Reader GET deployments
	curl -H "Authorization: Bearer t_reader" http://localhost:3000/deployments
- Admin POST deployment
	curl -H "Authorization: Bearer t_admin" -H 'content-type: application/json' -d '{"app_name":"demo","artifact_url":"file://foo"}' http://localhost:3000/deployments

Security note: Never commit real tokens; use environment/secret store. Tokens are hashed in-memory and only hash prefixes are logged at debug level.

