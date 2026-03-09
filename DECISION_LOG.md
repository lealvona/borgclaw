# Decision Log - BorgClaw Feature Completion

## D001: Treat Docs As Product Contract
**Decision**: Implement the runtime to match the current README and `docs/` feature set rather than pruning documented capabilities.

**Rationale**:
- The repository already advertises a clear product surface.
- Narrowing scope silently would create drift between documentation and behavior.
- The current module layout already reserves most of the needed interfaces.

**Alternatives Considered**:
- Prune undocumented or incomplete features from the plan.
- Treat the current repo as prototype-only and defer production behavior.

**Impact**:
- Planning and implementation are driven by docs parity.
- Missing features should be completed behind existing module boundaries.
- Documentation changes should follow implementation changes, not replace them.

---

## D002: Security-First Delivery Sequence
**Decision**: Sequence implementation by security and runtime correctness before breadth of integrations.

**Rationale**:
- The framework exposes command execution, remote channels, secrets, and external APIs.
- Approval, pairing, and policy enforcement need to exist before expanding autonomy.
- A stable agent core reduces rework across CLI, gateway, and channel integrations.

**Alternatives Considered**:
- Prioritize integration breadth first.
- Build transport breadth before core runtime maturity.

**Impact**:
- Phase 1 focuses on provider-backed execution and policy-aware tool plumbing.
- Transports and integrations should be wired through shared approval-aware runtime paths.
- Security policy remains a first-order constraint for subsequent features.
