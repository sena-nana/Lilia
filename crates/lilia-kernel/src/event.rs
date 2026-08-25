use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::FeatureId;

/// Declares one typed topic. Topics replace a single coarse enum so a consumer
/// reacting to project changes is never woken by terminal output.
pub trait Event: Clone + Send + Sync + 'static {
    const NAME: &'static str;
}

/// Identity of a subscription, used to detach a handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubscriptionId(u64);

/// Erased published event. Hosts that need to route across a thread boundary
/// observe envelopes and downcast to the topics they care about.
#[derive(Clone)]
pub struct EventEnvelope {
    sequence: u64,
    name: &'static str,
    type_id: TypeId,
    payload: Arc<dyn Any + Send + Sync>,
}

impl EventEnvelope {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn is<E: Event>(&self) -> bool {
        self.type_id == TypeId::of::<E>()
    }

    pub fn downcast<E: Event>(&self) -> Option<&E> {
        self.payload.downcast_ref::<E>()
    }
}

impl std::fmt::Debug for EventEnvelope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EventEnvelope")
            .field("sequence", &self.sequence)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

type TypedHandler = Arc<dyn Fn(&EventEnvelope) + Send + Sync>;

struct Handler {
    id: SubscriptionId,
    owner: Option<FeatureId>,
    call: TypedHandler,
}

#[derive(Default)]
struct EventBusState {
    typed: HashMap<TypeId, Vec<Handler>>,
    observers: Vec<Handler>,
}

/// Typed publish/subscribe with a monotonic sequence shared across topics.
///
/// Handlers run on the publishing thread; the bus releases its lock before
/// invoking them so a handler may publish again without deadlocking.
#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<EventBusInner>,
}

#[derive(Default)]
struct EventBusInner {
    state: Mutex<EventBusState>,
    next_sequence: AtomicU64,
    next_subscription: AtomicU64,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn publish<E: Event>(&self, event: E) -> EventEnvelope {
        let envelope = EventEnvelope {
            sequence: self.inner.next_sequence.fetch_add(1, Ordering::Relaxed) + 1,
            name: E::NAME,
            type_id: TypeId::of::<E>(),
            payload: Arc::new(event),
        };
        let (typed, observers) = {
            let state = self.state();
            let typed = state
                .typed
                .get(&TypeId::of::<E>())
                .map(|handlers| {
                    handlers
                        .iter()
                        .map(|handler| Arc::clone(&handler.call))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let observers = state
                .observers
                .iter()
                .map(|handler| Arc::clone(&handler.call))
                .collect::<Vec<_>>();
            (typed, observers)
        };
        for handler in typed {
            handler(&envelope);
        }
        for observer in observers {
            observer(&envelope);
        }
        envelope
    }

    pub fn on<E, F>(&self, owner: Option<FeatureId>, handler: F) -> SubscriptionId
    where
        E: Event,
        F: Fn(&E) + Send + Sync + 'static,
    {
        let id = self.next_subscription();
        let call: TypedHandler = Arc::new(move |envelope| {
            if let Some(event) = envelope.downcast::<E>() {
                handler(event);
            }
        });
        self.state()
            .typed
            .entry(TypeId::of::<E>())
            .or_default()
            .push(Handler { id, owner, call });
        id
    }

    /// Observes every topic. Used by a host that forwards envelopes onto its own
    /// dispatch loop rather than reacting inline.
    pub fn observe<F>(&self, owner: Option<FeatureId>, observer: F) -> SubscriptionId
    where
        F: Fn(&EventEnvelope) + Send + Sync + 'static,
    {
        let id = self.next_subscription();
        self.state().observers.push(Handler {
            id,
            owner,
            call: Arc::new(observer),
        });
        id
    }

    pub fn unsubscribe(&self, id: SubscriptionId) {
        let mut state = self.state();
        for handlers in state.typed.values_mut() {
            handlers.retain(|handler| handler.id != id);
        }
        state.observers.retain(|handler| handler.id != id);
    }

    pub(crate) fn unsubscribe_owner(&self, owner: &FeatureId) {
        let mut state = self.state();
        for handlers in state.typed.values_mut() {
            handlers.retain(|handler| handler.owner.as_ref() != Some(owner));
        }
        state
            .observers
            .retain(|handler| handler.owner.as_ref() != Some(owner));
    }

    pub fn sequence(&self) -> u64 {
        self.inner.next_sequence.load(Ordering::Relaxed)
    }

    fn next_subscription(&self) -> SubscriptionId {
        SubscriptionId(self.inner.next_subscription.fetch_add(1, Ordering::Relaxed) + 1)
    }

    fn state(&self) -> MutexGuard<'_, EventBusState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
