import { DefaultEventPriority } from "react-reconciler/constants.js";

import { runtime, send } from "./state.ts";
import {
	capped,
	CONTROLLED,
	diffProps,
	type Handler,
	type HostNode,
	type Json,
	type Props,
	propsOversized,
	ProtocolLimitError,
	type Session,
	TEXT,
	textOf,
	wireProps,
} from "./wire.ts";

const eventPriority = DefaultEventPriority ?? 32;

function bindHandlers(node: HostNode, props: Props): void {
	node.handlers = new Map();
	for (const [key, value] of Object.entries(props)) {
		if (typeof value === "function") node.handlers.set(key, value as Handler);
	}
}

export function blankNode(type: string): HostNode {
	return {
		id: 0,
		type,
		props: {},
		children: [],
		attached: false,
		handlers: new Map(),
	};
}

function serialize(session: Session, node: HostNode): Json {
	if (node.id === 0) node.id = session.nextId++;
	node.attached = true;
	session.nodes.set(node.id, node);
	return {
		id: node.id,
		type: node.type,
		props: node.props as Json,
		children: node.children.map((child) => serialize(session, child)),
	};
}

function forget(session: Session, node: HostNode): void {
	session.nodes.delete(node.id);
	for (const child of node.children) forget(session, child);
	node.children = [];
	node.handlers.clear();
	node.attached = false;
}

function place(
	session: Session,
	parent: HostNode,
	child: HostNode,
	before: HostNode | null,
): void {
	const existing = parent.children.indexOf(child);
	if (existing >= 0) parent.children.splice(existing, 1);
	const index = before
		? parent.children.indexOf(before)
		: parent.children.length;
	parent.children.splice(index < 0 ? parent.children.length : index, 0, child);
	if (child.attached) {
		session.ops.push({
			op: "move",
			parent: parent.id,
			id: child.id,
			before: before ? before.id : null,
		});
		return;
	}
	session.ops.push({
		op: "insert",
		parent: parent.id,
		before: before ? before.id : null,
		node: serialize(session, child),
	});
}

function unplace(session: Session, parent: HostNode, child: HostNode): void {
	const index = parent.children.indexOf(child);
	if (index >= 0) parent.children.splice(index, 1);
	if (child.attached && child.id !== 0) {
		session.ops.push({ op: "remove", parent: parent.id, id: child.id });
	}
	forget(session, child);
}

export function makeHost(session: Session): object {
	let priority = eventPriority;
	return {
		supportsMutation: true,
		supportsPersistence: false,
		supportsHydration: false,
		isPrimaryRenderer: true,
		noTimeout: -1,
		scheduleTimeout: setTimeout,
		cancelTimeout: clearTimeout,
		supportsMicrotasks: true,
		scheduleMicrotask: queueMicrotask,
		getRootHostContext: () => ({}),
		getChildHostContext: (context: unknown) => context,
		getPublicInstance: (instance: unknown) => instance,
		shouldSetTextContent: (type: string, props: Props) =>
			TEXT.has(type) && textOf(props.children) !== undefined,
		createInstance(type: string, props: Props) {
			const node = blankNode(type);
			node.props = wireProps(type, props);
			bindHandlers(node, props);
			return node;
		},
		createTextInstance(text: string) {
			const node = blankNode("label");
			node.props = { text: capped(text, 256) };
			return node;
		},
		appendInitialChild: (parent: HostNode, child: HostNode) => {
			parent.children.push(child);
		},
		appendChild: (parent: HostNode, child: HostNode) =>
			place(session, parent, child, null),
		appendChildToContainer: (parent: HostNode, child: HostNode) =>
			place(session, parent, child, null),
		insertBefore: (parent: HostNode, child: HostNode, before: HostNode) =>
			place(session, parent, child, before),
		insertInContainerBefore: (
			parent: HostNode,
			child: HostNode,
			before: HostNode,
		) => place(session, parent, child, before),
		removeChild: (parent: HostNode, child: HostNode) =>
			unplace(session, parent, child),
		removeChildFromContainer: (parent: HostNode, child: HostNode) =>
			unplace(session, parent, child),
		commitUpdate(node: HostNode, _type: string, prev: Props, next: Props) {
			bindHandlers(node, next);
			const diff = diffProps(node.type, prev, next);
			if (!diff) return;
			const op: { [key: string]: Json } = {
				op: "set",
				id: node.id,
				props: diff as Json,
			};
			const controlled = CONTROLLED[node.type]?.[1];
			if (
				node.confirmSeq !== undefined &&
				controlled &&
				Object.hasOwn(diff, controlled)
			) {
				op.seq = node.confirmSeq;
				node.confirmSeq = undefined;
			}
			node.props = wireProps(node.type, next);
			session.ops.push(op);
		},
		finalizeInitialChildren: () => false,
		prepareForCommit: () => null,
		resetAfterCommit() {
			session.pending.push(...session.ops);
			session.ops = [];
			if (!session.flushing && !session.scheduled) {
				session.scheduled = true;
				queueMicrotask(() => {
					session.scheduled = false;
					if (runtime.current === session) flushGuarded(session);
				});
			}
		},
		clearContainer: () => {},
		detachDeletedInstance: () => {},
		preparePortalMount: () => {},
		getCurrentEventPriority: () => eventPriority,
		setCurrentUpdatePriority: (next: number) => {
			priority = next;
		},
		getCurrentUpdatePriority: () => priority,
		resolveUpdatePriority: () => priority || eventPriority,
		shouldAttemptEagerTransition: () => false,
		maySuspendCommit: () => false,
		maySuspendCommitOnUpdate: () => false,
		maySuspendCommitInSyncRender: () => false,
		preloadInstance: () => true,
		startSuspendingCommit: () => {},
		suspendInstance: () => {},
		waitForCommitToBeReady: () => null,
		requestPostPaintCallback: () => {},
		trackSchedulerEvent: () => {},
		resolveEventType: () => null,
		resolveEventTimeStamp: () => -1.1,
		NotPendingTransition: null,
		HostTransitionContext: {
			$$typeof: Symbol.for("react.context"),
			_currentValue: null,
			_currentValue2: null,
		},
		resetFormInstance: () => {},
		commitMount: () => {},
		commitTextUpdate(node: HostNode, _previous: string, next: string) {
			const text = capped(next, 256);
			if (node.props.text === text) return;
			node.props.text = text;
			session.ops.push({ op: "set", id: node.id, props: { text } });
		},
		resetTextContent: () => {},
		hideInstance: () => {},
		hideTextInstance: () => {},
		unhideInstance: () => {},
		unhideTextInstance: () => {},
	};
}

function flushGuarded(session: Session): void {
	try {
		flush(session);
	} catch (error) {
		if (!(error instanceof ProtocolLimitError)) throw error;
		console.error(error);
		runtime.fail(70);
	}
}

export function flush(
	session: Session,
	fn?: () => void,
	emitCommit = true,
): void {
	session.flushing = true;
	try {
		flushInner(session, fn, emitCommit);
	} finally {
		session.flushing = false;
	}
}

function flushInner(
	session: Session,
	fn?: () => void,
	emitCommit = true,
): void {
	session.reconciler.flushSyncFromReconciler(fn);
	session.reconciler["flushPassiveEffects"]?.();
	for (const node of session.nodes.values()) {
		const controlled = CONTROLLED[node.type]?.[1];
		if (node.confirmSeq === undefined || !controlled) continue;
		session.pending.push({
			op: "set",
			id: node.id,
			props: { [controlled]: (node.props[controlled] ?? null) as Json },
			seq: node.confirmSeq,
		});
		node.confirmSeq = undefined;
	}
	let count = 0;
	const visit = (node: HostNode): void => {
		if (++count > 300) throw new ProtocolLimitError("tree exceeds 300 nodes");
		if (propsOversized(node.props)) {
			throw new ProtocolLimitError("array prop exceeds 64 items");
		}
		if (node.children.length > 200) {
			throw new ProtocolLimitError("parent exceeds 200 children");
		}
		for (const child of node.children) visit(child);
	};
	visit(session.container);
	if (session.pending.length > 1200) {
		throw new ProtocolLimitError("commit exceeds 1200 operations");
	}
	if (emitCommit && session.pending.length) {
		send({ t: "commit", ops: session.pending });
		session.pending = [];
	}
}
