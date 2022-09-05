//! A `QEMU`-based executor for binary-only instrumentation in `LibAFL`
use core::fmt::{self, Debug, Formatter};

use libafl::{
    bolts::shmem::ShMemProvider,
    events::{EventFirer, EventRestarter},
    executors::{Executor, ExitKind, HasObservers, InProcessExecutor, InProcessForkExecutor},
    feedbacks::Feedback,
    fuzzer::HasObjective,
    inputs::Input,
    observers::ObserversTuple,
    state::{HasClientPerfMonitor, HasSolutions},
    Error,
};

pub use crate::emu::SyscallHookResult;
use crate::{emu::Emulator, helper::QemuHelperTuple, hooks::QemuHooks};

pub struct QemuExecutor<'a, H, I, OT, QT, S>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
{
    hooks: &'a mut QemuHooks<'a, I, QT, S>,
    inner: InProcessExecutor<'a, H, I, OT, S>,
}

impl<'a, H, I, OT, QT, S> Debug for QemuExecutor<'a, H, I, OT, QT, S>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("QemuExecutor")
            .field("hooks", &self.hooks)
            .field("inner", &self.inner)
            .finish()
    }
}

impl<'a, H, I, OT, QT, S> QemuExecutor<'a, H, I, OT, QT, S>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
{
    pub fn new<EM, OF, Z>(
        hooks: &'a mut QemuHooks<'a, I, QT, S>,
        harness_fn: &'a mut H,
        observers: OT,
        fuzzer: &mut Z,
        state: &mut Self::State,
        event_mgr: &mut EM,
    ) -> Result<Self, Error>
    where
        EM: EventFirer + EventRestarter,
        OF: Feedback,
        Self::State: HasSolutions + HasClientPerfMonitor,
        Z: HasObservers,
    {
        Ok(Self {
            hooks,
            inner: InProcessExecutor::new(harness_fn, observers, fuzzer, state, event_mgr)?,
        })
    }

    pub fn inner(&self) -> &InProcessExecutor<'a, H, I, OT, S> {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut InProcessExecutor<'a, H, I, OT, S> {
        &mut self.inner
    }

    pub fn hooks(&self) -> &QemuHooks<'a, I, QT, S> {
        self.hooks
    }

    pub fn hooks_mut(&mut self) -> &mut QemuHooks<'a, I, QT, S> {
        self.hooks
    }

    pub fn emulator(&self) -> &Emulator {
        self.hooks.emulator()
    }
}

impl<'a, EM, H, I, OT, QT, S, Z> Executor for QemuExecutor<'a, H, I, OT, QT, S>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
{
    fn run_target<EM, Z>(
        &mut self,
        fuzzer: &mut Z,
        state: &mut Self::State,
        mgr: &mut EM,
        input: &Self::Input,
    ) -> Result<ExitKind, Error> {
        let emu = Emulator::new_empty();
        self.hooks.helpers_mut().pre_exec_all(&emu, input);
        let r = self.inner.run_target(fuzzer, state, mgr, input);
        self.hooks.helpers_mut().post_exec_all(&emu, input);
        r
    }
}

impl<'a, H, I, OT, QT, S> HasObservers for QemuExecutor<'a, H, I, OT, QT, S>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
{
    #[inline]
    fn observers(&self) -> &OT {
        self.inner.observers()
    }

    #[inline]
    fn observers_mut(&mut self) -> &mut OT {
        self.inner.observers_mut()
    }
}

pub struct QemuForkExecutor<'a, H, I, OT, QT, S, SP>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
    SP: ShMemProvider,
{
    hooks: &'a mut QemuHooks<'a, I, QT, S>,
    inner: InProcessForkExecutor<'a, H, I, OT, S, SP>,
}

impl<'a, H, I, OT, QT, S, SP> Debug for QemuForkExecutor<'a, H, I, OT, QT, S, SP>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
    SP: ShMemProvider,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("QemuForkExecutor")
            .field("hooks", &self.hooks)
            .field("inner", &self.inner)
            .finish()
    }
}

impl<'a, H, I, OT, QT, S, SP> QemuForkExecutor<'a, H, I, OT, QT, S, SP>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
    SP: ShMemProvider,
{
    pub fn new<EM, OF, Z>(
        hooks: &'a mut QemuHooks<'a, I, QT, S>,
        harness_fn: &'a mut H,
        observers: OT,
        fuzzer: &mut Z,
        state: &mut Self::State,
        event_mgr: &mut EM,
        shmem_provider: SP,
    ) -> Result<Self, Error>
    where
        EM: EventFirer + EventRestarter,
        OF: Feedback,
        Self::State: HasSolutions + HasClientPerfMonitor,
        Z: HasObservers,
    {
        assert!(!QT::HOOKS_DO_SIDE_EFFECTS, "When using QemuForkExecutor, the hooks must not do any side effect as they will happen in the child process and then discarded");

        Ok(Self {
            hooks,
            inner: InProcessForkExecutor::new(
                harness_fn,
                observers,
                fuzzer,
                state,
                event_mgr,
                shmem_provider,
            )?,
        })
    }

    pub fn inner(&self) -> &InProcessForkExecutor<'a, H, I, OT, S, SP> {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut InProcessForkExecutor<'a, H, I, OT, S, SP> {
        &mut self.inner
    }

    pub fn hooks(&self) -> &QemuHooks<'a, I, QT, S> {
        self.hooks
    }

    pub fn hooks_mut(&mut self) -> &mut QemuHooks<'a, I, QT, S> {
        self.hooks
    }

    pub fn emulator(&self) -> &Emulator {
        self.hooks.emulator()
    }
}

impl<'a, EM, H, I, OT, QT, S, Z, SP> Executor for QemuForkExecutor<'a, H, I, OT, QT, S, SP>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
    SP: ShMemProvider,
{
    fn run_target<EM, Z>(
        &mut self,
        fuzzer: &mut Z,
        state: &mut Self::State,
        mgr: &mut EM,
        input: &Self::Input,
    ) -> Result<ExitKind, Error> {
        let emu = Emulator::new_empty();
        self.hooks.helpers_mut().pre_exec_all(&emu, input);
        let r = self.inner.run_target(fuzzer, state, mgr, input);
        self.hooks.helpers_mut().post_exec_all(&emu, input);
        r
    }
}

impl<'a, H, I, OT, QT, S, SP> HasObservers for QemuForkExecutor<'a, H, I, OT, QT, S, SP>
where
    H: FnMut(&Self::Input) -> ExitKind,
    I: Input,
    OT: ObserversTuple,
    QT: QemuHelperTuple<I, S>,
    SP: ShMemProvider,
{
    #[inline]
    fn observers(&self) -> &OT {
        self.inner.observers()
    }

    #[inline]
    fn observers_mut(&mut self) -> &mut OT {
        self.inner.observers_mut()
    }
}
