-- The fence that makes one logical effect at most one.
--
-- Sling delivers at least once and a cluster runs more than one daemon, so the
-- question "may I run this now" has to be answered by a durable write rather
-- than by whichever process asked most recently. Three columns answer it.
--
-- The fence names the worker that holds the right to start. The checkpoint
-- records that starting has happened, and once it is set nothing sets it again:
-- not a lease expiring, not a physical requeue, not a restart, not a node being
-- replaced. After it, an unresolved outcome is unresolved, and the honest
-- answer is that nobody knows rather than a second attempt.
--
-- The attempt count is the outbox's, not Sling's. Several physical records
-- carrying one logical operation is ordinary; several logical attempts at one
-- command effect is not, and this is what bounds them.

ALTER TABLE agent_operation ADD COLUMN execution_checkpoint TEXT;

ALTER TABLE agent_operation ADD COLUMN outbox_attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE agent_operation ADD COLUMN worker_fence INTEGER;
