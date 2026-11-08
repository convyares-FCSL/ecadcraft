# ECAD architecture foundations

12 October 2026 · Design direction for Claude following Phase 0

## Purpose and immediate priority

Establish the document model and the `.ecad` / `.ecadlib` contracts before implementing drafting features, MCP or the cable demonstration. CADCraft supplies drawing capabilities; it must not determine the electrical domain model. The priority is a durable foundation that lets a panel designer start drawing immediately and add detail progressively.

This note consolidates decisions from Carl's architecture discussion. It refines the concept spec; it does not approve a final schema, trait hierarchy or fork implementation. Where the earlier spec assumes automatic component creation, globally unique tags or one wire number, use the distinctions below instead.

## Agreed foundations

### Geometry can exist without electrical meaning

Ordinary lines, rectangles and circles are valid drawing content, with no component required. Selecting geometry and choosing **Convert to component** creates a component identity linked to the existing geometry, preserving its appearance and position.

A component may initially have no description, manufacturer, part number or terminals. These can be added or changed later. Copy/paste and dragging from a side panel are important normal workflows; replacing a representation from a library is a later interaction.

Drawing with a dedicated wire tool creates a conductor identity and a drawing representation immediately. Loose ends and unspecified conductor properties are valid. Ordinary line drawing must remain possible without creating conductors.

### Identity is independent of names

Components, conductors and terminals need stable internal identities. References use those identities, not editable tags or terminal labels. A rename must not break connectivity. GUIDs are a candidate encoding, not a requirement to expose unreadable identifiers to users.

Conductors and components are both central domain objects. Do not require a separate fully specified physical-wire object before a conductor can exist.

### Electrical meaning and drawing representation are separate

| Domain meaning | Drawing representation |
| --- | --- |
| Component, with terminals added as needed | One or more symbols or groups of geometry |
| Conductor, with optional endpoints and properties | Wire route and associated annotations |
| Cable grouping conductor references | Slash, bundled line or fan-out |

Changing route bends to avoid clashes preserves conductor identity and connectivity. Reconnecting an endpoint changes electrical intent. The gesture can be natural, but the saved change must distinguish rerouting from reconnection.

Geometry, routing and deliberate annotation placement are document content, not disposable UI state. The same electrical meaning may have different representations; switching cable views must not duplicate conductors or change connections.

### A cable groups conductors

A cable references existing conductors and holds shared cable information. Grouping wires into a cable preserves their conductor identities. No separate cable-core entity is required merely to duplicate a conductor: “core” describes a conductor's role within a cable. A core designation can be recorded on the cable membership if needed.

A net represents electrical connectivity across conductors and termination points. Cable membership is a design grouping; net membership follows connectivity. Exact rules for junctions, terminal bridges and net identity remain to be designed.

### Incomplete designs are supported

No mandatory wire-size setup, numbering policy, component catalogue selection or library administration should block initial drawing. Missing information must remain distinguishable from confirmed values and be queryable.

Later, heat maps and queries should expose completeness and explain what is missing. Red/yellow/green is an illustrative presentation, not a schema. Completeness should be calculated from explicit criteria; it is not proof of electrical correctness. Criteria, colours and UI are deferred.

## Foundation direction to develop

The operation log records accepted edits. Applying those edits creates the current document objects, including ordinary geometry and domain objects. The objects are the materialised document state, not a competing independently saved truth. Drawing adapters and exporters consume that state.

Record resolved intent rather than raw mouse gestures: a selected terminal reference must not be reinterpreted by a newer snapping algorithm during replay. Compound edits need atomic transaction semantics so domain objects and representations cannot be left half-updated.

Rust traits can express shared capabilities across different object types. Use them where behaviour is shared; cable membership and representation links are relationships, not inheritance. Concrete types, typed IDs and the trait boundaries should follow the agreed model rather than precede it.

## Deliberately open decisions

| Subject | Constraint to preserve; decision still needed |
| --- | --- |
| Copy and duplicate | Independent copies normally need new component IDs, but duplicating a contact may add a representation/contact to the existing contactor. Distinguish another physical contact from another depiction of the same contact. Do not hard-code one meaning for all copying. |
| Wire identification | Support the possibility of manual or automatic identifiers, net-based or conductor-based numbering, different destination markings at the two ends, repeated labels and hidden labels. No universal single printed wire number assumption. Actual schemes are deferred. |
| Tags and scope | Stable IDs are authoritative. Determine tag scope and ambiguity handling before implementing “same tag means same component”. |
| Library contract | Retain the concept's self-contained project and explicit library-update intent. Define reusable definitions versus project instances, embedded versions, terminal mapping and behaviour on replacement. Decide whether parts share the library container later. |
| History and storage | Resolve checkpoint authority after compaction, preservation of named revisions, transaction recovery, undo and operation evolution before freezing the file format. |
| Inference evolution | Define how changed inference rules affect old projects and released revisions. Explicit user overrides alone do not guarantee unchanged inferred results. |
| Engine ownership | Demonstrate how CADCraft edits enter the document transaction path and how projection/undo remain consistent. Do not allow two competing authoritative models. |

## Requested next output from Claude

Produce a document-model proposal for review before feature implementation: show the minimal objects and relationships, what is stored versus derived, and how project objects relate to library definitions and CADCraft entities. Use a small worked example covering raw geometry converted to a component, a loose conductor later connected and grouped into a cable, and a route edit that preserves connectivity.

Identify unresolved choices explicitly. Do not silently settle copying, numbering or library replacement behaviour while implementing the example. The next approval is for the document architecture, not the cable demo or broader Phase 1 features.
