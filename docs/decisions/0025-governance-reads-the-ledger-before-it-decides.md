# 0025 · Governance reads the ledger before it decides

- **Status**: cut 1 implemented (#434) · migration 0035 adds `knowledge_bases.governance` and `agent_decisions`; `governance` in the store holds the precedent families, the first-in-first-out queue with its clusters, the gate and the table; the `govern` job in the server reads the switch, calls the model with precedents and applies or proposes; `?queue=agent`, `ReviewCounts.agent` and `POST /kbs/{id}/review/agent/{decision_id}` on the API · cut 3 (UI, #437) implemented: the switch in base settings, the Agent queue with its rows and answers, the proposal chip on a duplicate card whose Merge / Keep answers the proposal, the Agent section on the Overview · cut 2 implemented: migration 0036 adds `question`, `trace` and `calls` to `agent_decisions`; `governor` in the extract crate holds the tools, the opening and the step reader; `investigate` in the server job runs the loop for pairs the batch could not settle (decision 8) · cut 4 implemented: migration 0037 adds `knowledge_bases.governance_since`; `fuse` in the server job turns the switch off after two reverts of the agent's merges since it was turned on, within seven days, raises `governance.tripped` and writes the ledger (decision 9)
- **Written**: 2026-09-06 (conventions in the [README](README.md))
- **Related**: [0016](0016-close-the-open-seams-before-cutting-new-ones.md) C2 gave a base its first automation switch, `auto_type_resolution`, and this record copies its shape; #428 asked for bulk and automatic handling of same-name pairs and got the batch path (#429, #430) this builds on; [0020](0020-an-auditor-reads-it-without-us.md) made the ledger complete enough to be read back; [0015](0015-recording-a-sentence-is-not-asserting-a-fact.md) keeps a person's own sentences out of any machine's reach.

> Three hundred namesake pairs wait in Review. A person merges the first ten "Apple / Apple Inc." pairs and keeps the first ten "Zhang Wei / Zhang Wei" pairs apart, and the remaining two hundred and eighty wait for the same person to do the same thing two hundred and eighty more times. The record of those twenty decisions is in the ledger, with names, scores, times and who. Nothing reads it.

## What the adjudicator does today

The resolver files a grey-zone pair as `resolution_reviews` in stage `adjudicating`. The adjudicator (`adjudication.rs`) sends twelve pairs per model call with each side's name, type and top facts, caches the verdict under a key of names, types and facts, and at confidence 0.8 merges or keeps; anything else is escalated to stage `human` with a reason code. From then on the machine never looks at the pair again. The prompt has no history: the same "Zhang Wei" a person has kept apart three times is judged from zero the fourth time, and the pair a person merged yesterday is judged from zero today.

The people using a base have meanwhile written every decision they made into `audit_events`: `review.merge` and `review.keep` with both names, `merge.manual`, `merge.revert`, and the `fact.*` and `conflict.*` families for the other queues. Every row carries who and when. That is the precedent.

## Decisions

### 1. The ledger is the precedent, and only people write it

For a pair the governor asks the ledger four questions: what people decided on these two names (either order), what people decided about either name against other names, how this base decides pairs of this type pair (merged, kept, later reverted), and whether a merge involving either name was reverted. Only rows with an actor count. The agent's own rows never count, or it would cite itself and grow more confident with every round. A proposal a person accepted counts, because the acceptance goes through the person's decide path and is recorded as that person's decision (decision 6). The retrieved precedents go into the adjudication prompt as lines in plain English, and the model is asked to say in one sentence which one it relied on.

### 2. A switch per base, off by default; on starts the queue, off stops it

`knowledge_bases.governance`, a boolean, next to `auto_type_resolution`. Turning it on enqueues a `govern` job at once; an extraction that leaves pairs behind enqueues one; a person's decision enqueues one; an hourly sweep enqueues one for every base whose queue holds pairs the agent has not looked at. All four go through `enqueue_unless_queued`, so a base has at most one job waiting. The job re-reads the switch before every cluster and returns when it is off: turning the switch off ends the queue at the next cluster boundary, and nothing already decided is undone.

Off is the default for the opposite reason `auto_type_resolution` defaults to on: that one was measured (39/41) and moves a type one step down its own subtree, this one merges entities and has not been measured on any base yet. A base earns automation by having history to read, and its owner turns it on.

Two decisions folded into this one. The record first had three states, off / suggest / auto. The gate (decision 4) already turns everything that does not clear it into a proposal, so "suggest" is what the switch does for the pairs it cannot decide, and a second knob would explain the same thing twice. The sweep is hourly because the instant triggers cover the live cases; the sweep exists for backlog left after a job's retries ran out, and a model that is down should not be hit every minute.

### 3. First in, first out, and the head brings its cluster

The queue is every pending pair without an open proposal, oldest first. The head's cluster is every other queued pair that shares a name or an entity with it, up to twelve in all, and the whole cluster goes into one model call. Oldest first because the oldest has waited longest and because the order is one a person can predict; the cluster because a name should be judged consistently in one sitting, not once per pair across separate calls, and because the cluster is the unit #428 asked about: three "Zhang Wei" pairs a person kept apart and nine more that should follow.

### 4. Merging needs precedent, keeping apart needs confidence, hard rules come first

The gate is a pure function of the verdict, its confidence, whether the two types conflict, and the precedents. In order: an `unsure` verdict is proposed; confidence below 0.85 is proposed; any reverted merge involving either name is proposed; a verdict that contradicts what a person decided on this same pair is proposed. A `different` verdict that survives those is applied. A `same` verdict is proposed when the types conflict, and otherwise applied only with support: this pair was merged before, this name was merged before and never kept apart, or this type pair has at least five human decisions, merges at least as many as keeps, and no reverts. A base with no human history therefore never merges on its own; it proposes, and the answers become the history.

Keeping apart is cheap: it changes no fact and a manual merge undoes it. Merging moves facts and a revert has to unwind them, so it is the side that needs a person to have gone first. The threshold sits above the adjudicator's 0.8 because the adjudicator judges one pair in isolation and the governor is standing in for a person.

### 5. Every look is a row in `agent_decisions`

One row per pair the agent looked at: the pair, the action (`merge`, `keep`, `unsure`), the confidence, the model's sentence, the precedents it was shown, and a status. `proposed` waits for a person; `applied` was decided and can be reverted; `accepted`, `overridden` and `reverted` are a person's answers; `superseded` means a person decided a sibling (same name or same entity) after the proposal was written, so the proposal is stale and the pair returns to the queue to be looked at again with the new precedent. That last transition is the cascade: a person keeps one "Zhang Wei" pair apart, the agent's open proposals on the other "Zhang Wei" pairs are superseded, the next job finds them at the head of the queue with a same-pair precedent, and the gate lets it keep them apart itself.

A partial unique index allows one open proposal per pair. The table is the Agent queue on the Review page and the source of the acceptance, override and revert counts; nothing has to be reconstructed from the ledger.

### 6. Answers go through the person's path

Accepting or overriding a proposal calls `decide_review` with the person's id, exactly as the Review page does; reverting an applied merge calls `revert_merge`; merging over an applied keep calls `merge_entities` with the person as `merged_by`. Each writes the usual audit row plus the decision id. So an answer is a person's decision in every table that records decisions, and by decision 1 it is precedent for the next look.

### 8. A pair the batch cannot settle is looked at again, with tools, and then decided or asked about

The batch judges twelve pairs from their top facts and the precedent lines, and its verdict is `unsure` or under the threshold often enough that a base's proposals would otherwise be mostly shrugs. Those pairs, and only those, go through a second look: one pair per conversation, with tools. The model may ask for the full facts of a side, the source passages the facts came from (with the document each is in), what people decided about any name (a substring search of the ledger, so "Apple Computer" turns up for "Apple"), and other entities named alike. At most six lookups, then one of two endings: `decide` with a verdict, a confidence and one sentence naming the fact, passage or precedent that settled it, or `defer` with the one question a person could answer at a glance, naming the fact or document that would settle it. The verdict goes through the same gate as the batch's; nothing the loop finds lets it merge without precedent or against a rule. The question is stored on the row and shown as the row's text; the lookups are stored as a trace and shown under the row, so the explanation of a proposal is what the agent looked at.

Pairs the hard rules stop never enter the loop: a type conflict, a reverted merge in the history, a same-pair precedent that contradicts the verdict. More looking would not change a rule. A base has a daily budget of loop calls (three hundred); when it is spent, uncertain pairs are proposed from the batch's verdict as in cut 1, so the loop can never run away on a large backlog. A model that stops calling tools is nudged once and then treated as not having concluded, with the trace kept.

### 9. Two reverts in a week turn the switch off

A person reverting one of the agent's merges is the strongest signal the ledger has: the agent stood in for a person and was wrong in the one way that costs work to undo. Two of them within seven days, counted from the moment the switch was last turned on, turn the switch off. The base gets a `governance.tripped` alert for its editors and a `kb.updated` row with the machine as the actor, so the switch-off is in the ledger like every other decision. The queue stops at its next cluster boundary as it does for any switch-off; proposals already made stay for people to answer. Turning the switch back on records `governance_since` afresh, so the two reverts that tripped it do not trip it again on the first mistake after a restart.

Both revert paths count: the Agent queue's Revert and the Merges page's Revert on a merge the agent made (the latter now marks the agent's row `reverted` too, so the queue and the history agree). A person merging over an applied keep, or overriding a proposal, does not count: those are corrections to a suggestion, not to a change in the graph.

### 7. The governor does not touch the verdict cache

The adjudicator's cache is keyed on names, types and facts. The governor's answer also depends on the precedents, which change with every human decision; a cached verdict would be one taken without them. The governor neither reads nor writes the cache, and the two are kept apart by the switch: off, the adjudicator runs the adjudicating stage as before; on, extraction enqueues the governor instead and both stages flow through the one queue.

## What a reader sees

Duplicates that the agent proposed on carry the reason code `proposed`; pairs it decided are closed with `governed|<confidence>` and appear under Merges and in History with the machine as the actor, as auto-merges always have. `?queue=agent` lists the rows of decision 5 newest first with both names, the reason and the precedents. The cut-3 UI puts the switch in the base settings, the queue in the Review rail and the counts on the Overview.

## Dead ends

- **Suggestion columns on `resolution_reviews`.** Four columns for action, confidence, reason and precedents. They cover only duplicates, keep no record of how a person answered, and cannot express "superseded"; the table is one more table and answers all three.
- **Three states: off, suggest, auto.** See decision 2. The gate makes suggesting the fallback for anything it will not apply, so the middle state was already there.
- **Precedent by embedding.** Embed each decided pair and retrieve the nearest. Name and type families already cover what actually recurs, and the nearest neighbour brings back pairs that look alike and were decided for reasons the vector cannot see. Reconsider when cut 2 gives the model a search tool it can steer.
- **A minute-level sweep.** The other schedulers tick every minute. A dead model would then be called once a minute per base with backlog until someone noticed.

## Open questions

- **Other queues.** Conflicts have three human actions to learn from (`conflict.close_old`, `keep_both`, `reject_new`); unconfirmed and low-confidence facts have `fact.confirm` and `fact.reject`. Nods (`pending_facts`, 0015) stay out: the person who said it nods.
- **Cross-base precedent.** A workspace's other bases may hold the same names. Kept to one base until someone asks.
- **A budget for the batch.** The loop has a daily budget (decision 8); the batch does not. A job runs twenty clusters and re-enqueues itself while backlog remains, which is what the adjudicator always did. Whether a base needs a cap on that too is a question for a large corpus.
- **Tools the loop does not have yet.** The graph around a side beyond its direct facts, the entity's disambiguator history, a full-text search of the corpus. Add one when a deferred question keeps asking for it.
