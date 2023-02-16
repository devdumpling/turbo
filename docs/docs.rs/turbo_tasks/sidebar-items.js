window.SIDEBAR_ITEMS = {"attr":[["function",""],["value","Creates a ValueVc struct for a `struct` or `enum` that represent that type placed into a cell in a Task."],["value_impl",""],["value_trait",""]],"enum":[["RawVc",""],["ResolveTypeError",""],["StatsType","The type of stats reporting."],["TaskInput",""]],"fn":[["dynamic_call","see [TurboTasks] `dynamic_call`"],["emit",""],["get_invalidator","Get an [Invalidator] that can be used to invalidate the current [Task] based on external events."],["mark_stateful","Marks the current task as stateful. This prevents the tasks from being dropped without persisting the state."],["register",""],["run_once",""],["spawn_blocking",""],["spawn_thread",""],["trait_call","see [TurboTasks] `trait_call`"],["turbo_tasks",""],["with_task_id_mapping",""],["without_task_id_mapping",""]],"mod":[["backend",""],["debug",""],["event",""],["graph",""],["persisted_graph",""],["primitives",""],["registry",""],["small_duration",""],["test_helpers",""],["trace",""],["util",""]],"struct":[["CellId",""],["CollectiblesFuture",""],["Completion","Just an empty type, but it’s never equal to itself. [CompletionVc] can be used as return value instead of `()` to have a concrete reference that can be awaited. It will invalidate the awaiting task everytime the referenced task has been executed."],["CompletionVc","Vc for [`Completion`]"],["CompletionsVc","Vc for [`Completions`]"],["Error","The `Error` type, a wrapper around a dynamic error type."],["FunctionId",""],["Invalidator",""],["NativeFunction","A native (rust) turbo-tasks function. It’s used internally by `#[turbo_tasks::function]`."],["NativeFunctionVc","Vc for [`NativeFunction`]"],["Nothing","Just an empty type. [NothingVc] can be used as return value instead of `()` to have a concrete reference that can be awaited."],["NothingVc","Vc for [`Nothing`]"],["ReadRawVcFuture",""],["ReadRef","The read value of a value cell. The read value is immutable, while the cell itself might change over time. It’s basically a snapshot of a value at a certain point in time."],["SharedReference",""],["SharedValue",""],["State",""],["TaskId",""],["TraitType",""],["TraitTypeId",""],["TransientInstance","Pass a reference to an instance to a turbo-tasks function."],["TransientValue","Pass a value by value (`Value<Xxx>`) instead of by reference (`XxxVc`)."],["TurboTasks",""],["Value","Pass a value by value (`Value<Xxx>`) instead of by reference (`XxxVc`)."],["ValueToStringVc",""],["ValueType","A definition of a type of data."],["ValueTypeId",""]],"trait":[["CollectiblesSource",""],["FromSubTrait",""],["FromTaskInput",""],["IdMapping",""],["IntoSuperTrait",""],["JoinIterExt",""],["TaskIdProvider",""],["TraitMethod",""],["TryJoinIterExt",""],["TurboTasksApi",""],["TurboTasksBackendApi",""],["TurboTasksCallApi",""],["Typed",""],["TypedForInput","Marker trait that a turbo_tasks::value is prepared for serialization as Value<…> input. Either use `#[turbo_tasks::value(serialization: auto_for_input)]` or avoid Value<…> in favor of a real Vc"],["ValueToString",""],["ValueTraitVc",""],["ValueVc",""]],"type":[["Result","`Result<T, Error>`"]]};