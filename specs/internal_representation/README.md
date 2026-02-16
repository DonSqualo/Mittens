# Internal Representation Specs

This folder defines the first implementation pass for a canonical geometry IR in Mittens.

Scope of this pass:

- stabilize geometry semantics from existing Lua authoring patterns
- introduce a fast IR loop for one real model (`pure_acoustics.lua`)
- keep existing rendering/export flow working while adding IR artifacts

Files:

- `conventions.md` - conventions extracted from current scripts and stdlib behavior
- `plan.md` - implementation plan with phased milestones
- `subagent_quick_loop.md` - first executable loop spec and usage
