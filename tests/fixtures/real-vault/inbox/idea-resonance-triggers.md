---
title: Idea — Resonance Triggers for Workflows
tags: [inbox, idea, workflows]
status: raw
---

# Idea — Resonance Triggers for Workflows

Phase 15 wants workflows that auto-suggest themselves to the user
when the cognitive state aligns with the workflow's trigger vector.

The mechanism:

```
for workflow in enabled_workflows:
    sim = cosine(cognitive_state, workflow.trigger_vector)
    if sim > workflow.threshold and (now - workflow.last_fired) > cooldown:
        suggest_to_user(workflow)
```

That's it. The cleverness is entirely in **how the trigger_vector
gets built**. Two options:
1. User authors it explicitly via the WorkflowBuilder UI.
2. Auto-derived from past successful runs: take the cortex state at
   each `accepted=true` run, average + normalize.

Option 2 lets the user "teach" workflows by example without any prompt
engineering. It's basically online classification with a 1-shot prior.

## Crosslinks

- The cortex state these dot against: [[projects/aura/cognitive-core-design]].
- The workflows we ship by default: [[projects/aura/roadmap|Phase 15]].
