use crate::{
    error,
    maybe_trace::MaybeTrace,
    ui::{
        component::{Component, Event, EventCtx},
        flow::base::FlowMsg,
        geometry::Rect,
        shape::Renderer,
    },
};

use crate::{
    micropython::gc::Gc,
    ui::{component::swipe_detect::SwipeConfig, flow::Swipable},
};

/// `FlowStore` is essentially `Vec<Gc<dyn Component + SimpleSwipable>>` except
/// that `trait Component` is not object-safe so it ends up being a kind of
/// recursively-defined tuple.
/// Implementors are something like the V in MVC.
pub trait FlowStore {
    /// Call `Component::place` on all elements.
    fn place(&mut self, bounds: Rect) -> Rect;

    /// Call `Component::event` on i-th element.
    fn event(&mut self, i: usize, ctx: &mut EventCtx, event: Event) -> Option<FlowMsg>;

    /// Call `Component::render` on i-th element.
    fn render<'s>(&'s self, i: usize, target: &mut impl Renderer<'s>);

    #[cfg(feature = "ui_debug")]
    /// Call `Trace::trace` on i-th element.
    fn trace(&self, i: usize, t: &mut dyn crate::trace::Tracer);

    /// Forward `SimpleSwipable` methods to i-th element.
    fn map_swipable<T>(&mut self, i: usize, func: impl FnOnce(&mut dyn Swipable) -> T) -> T;

    fn get_swipe_config(&self, i: usize) -> SwipeConfig;

    fn get_internal_page_count(&mut self, i: usize) -> usize;

    /// Add a Component to the end of a `FlowStore`.
    fn add<E: Component<Msg = FlowMsg> + MaybeTrace + Swipable>(
        self,
        elem: E,
    ) -> Result<impl FlowStore, error::Error>
    where
        Self: Sized;
}

/// Create new empty flow store.
pub fn flow_store() -> impl FlowStore {
    FlowEmpty {}
}

/// Terminating element of a recursive structure.
struct FlowEmpty;

// Methods that take an index panic because it's always out of bounds.
impl FlowStore for FlowEmpty {
    fn place(&mut self, bounds: Rect) -> Rect {
        bounds
    }

    fn event(&mut self, _i: usize, _ctx: &mut EventCtx, _event: Event) -> Option<FlowMsg> {
        panic!()
    }

    fn render<'s>(&self, _i: usize, _target: &mut impl Renderer<'s>) {
        panic!()
    }

    #[cfg(feature = "ui_debug")]
    fn trace(&self, _i: usize, _t: &mut dyn crate::trace::Tracer) {
        panic!()
    }

    fn map_swipable<T>(&mut self, _i: usize, _func: impl FnOnce(&mut dyn Swipable) -> T) -> T {
        panic!()
    }

    fn add<E: Component<Msg = FlowMsg> + MaybeTrace + Swipable>(
        self,
        elem: E,
    ) -> Result<impl FlowStore, error::Error>
    where
        Self: Sized,
    {
        Ok(FlowComponent2 {
            elem: Gc::new(elem)?,
            next: Self,
        })
    }
    fn get_swipe_config(&self, _i: usize) -> SwipeConfig {
        SwipeConfig::new()
    }
    fn get_internal_page_count(&mut self, _i: usize) -> usize {
        1
    }
}

struct FlowComponent2<E: Component<Msg = FlowMsg>, P> {
    /// Component allocated on micropython heap.
    pub elem: Gc<E>,

    /// Nested FlowStore.
    pub next: P,
}

impl<E: Component<Msg = FlowMsg>, P> FlowComponent2<E, P> {
    fn as_ref(&self) -> &E {
        &self.elem
    }

    fn as_mut(&mut self) -> &mut E {
        // SAFETY: micropython can only access this object through LayoutObj which wraps
        // us in a RefCell which guarantees uniqueness
        unsafe { Gc::as_mut(&mut self.elem) }
    }
}

impl<E, P> FlowStore for FlowComponent2<E, P>
where
    E: Component<Msg = FlowMsg> + MaybeTrace + Swipable,
    P: FlowStore,
{
    fn place(&mut self, bounds: Rect) -> Rect {
        self.as_mut().place(bounds);
        self.next.place(bounds);
        bounds
    }

    fn event(&mut self, i: usize, ctx: &mut EventCtx, event: Event) -> Option<FlowMsg> {
        if i == 0 {
            self.as_mut().event(ctx, event)
        } else {
            self.next.event(i - 1, ctx, event)
        }
    }

    fn render<'s>(&'s self, i: usize, target: &mut impl Renderer<'s>) {
        if i == 0 {
            self.as_ref().render(target)
        } else {
            self.next.render(i - 1, target)
        }
    }

    #[cfg(feature = "ui_debug")]
    fn trace(&self, i: usize, t: &mut dyn crate::trace::Tracer) {
        if i == 0 {
            self.as_ref().trace(t)
        } else {
            self.next.trace(i - 1, t)
        }
    }

    fn map_swipable<T>(&mut self, i: usize, func: impl FnOnce(&mut dyn Swipable) -> T) -> T {
        if i == 0 {
            func(self.as_mut())
        } else {
            self.next.map_swipable(i - 1, func)
        }
    }

    fn add<F: Component<Msg = FlowMsg> + MaybeTrace + Swipable>(
        self,
        elem: F,
    ) -> Result<impl FlowStore, error::Error>
    where
        Self: Sized,
    {
        Ok(FlowComponent2 {
            elem: self.elem,
            next: self.next.add(elem)?,
        })
    }

    fn get_swipe_config(&self, i: usize) -> SwipeConfig {
        if i == 0 {
            self.as_ref().get_swipe_config()
        } else {
            self.next.get_swipe_config(i - 1)
        }
    }

    fn get_internal_page_count(&mut self, i: usize) -> usize {
        self.map_swipable(i, |swipable| swipable.get_internal_page_count())
    }
}
