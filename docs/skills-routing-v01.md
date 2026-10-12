# Task Skills routing and observed-use log

Per user turn, ChatCMD matches task keywords against enabled skills and returns up to three explainable suggestions in agent_user_message.skillRouting. Matching is heuristic and advisory; it never auto-executes scripts or changes tool permissions.

Idempotent timeline status events record recommendations without user text. Only successful skill_read calls record observed reads, without storing SKILL.md content. The authenticated local GET /api/local/tasks/{id}/skill-routing endpoint returns latest suggestions, actual reads and recent history. The task UI polls read-only every 15 seconds while visible.

Suggested means matched; read means tool invocation succeeded; neither proves the assistant followed instructions or completed the workflow. Adherence remains notVerified. No database migration required.
