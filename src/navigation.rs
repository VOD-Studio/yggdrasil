//! Dioxus History 适配器。浏览器端由 JS 在旧快照完成后提交路由，SSR 使用原 History。

use dioxus::history::History;
use std::rc::Rc;

pub fn transition_history(inherited: Rc<dyn History>) -> Rc<dyn History> {
    #[cfg(target_arch = "wasm32")]
    if let Some(history) = browser::TransitionHistory::connect(inherited.current_prefix()) {
        return Rc::new(history);
    }
    inherited
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use crate::bridges::navigation::wasm::{get_module, NavigationModule};
    use std::{cell::RefCell, sync::Arc};
    use wasm_bindgen::prelude::Closure;

    type Updater = Arc<dyn Fn() + Send + Sync>;

    pub struct TransitionHistory {
        module: NavigationModule,
        updater: Rc<RefCell<Option<Updater>>>,
        // JS 持有函数引用；在 disconnect 后才释放闭包，避免悬垂调用。
        _notify: Closure<dyn FnMut()>,
    }

    impl TransitionHistory {
        pub fn connect(prefix: Option<String>) -> Option<Self> {
            let module = get_module()?;
            let updater = Rc::new(RefCell::new(None::<Updater>));
            let notify = {
                let updater = updater.clone();
                Closure::wrap(Box::new(move || {
                    let callback = updater.borrow().clone();
                    if let Some(callback) = callback {
                        callback();
                    }
                }) as Box<dyn FnMut()>)
            };
            if module.connect(&notify, prefix.as_deref()).is_err() {
                let _ = module.disconnect();
                return None;
            }
            Some(Self {
                module,
                updater,
                _notify: notify,
            })
        }
    }

    impl History for TransitionHistory {
        fn current_route(&self) -> String {
            // Router::push 会同步请求重渲染，这里必须保持旧路由直到 VT 回调提交。
            self.module.current_route()
        }

        fn current_prefix(&self) -> Option<String> {
            self.module.current_prefix()
        }

        fn go_back(&self) {
            self.module.back();
        }

        fn go_forward(&self) {
            self.module.forward();
        }

        fn push(&self, route: String) {
            self.module.push(&route);
        }

        fn replace(&self, route: String) {
            self.module.replace(&route);
        }

        fn external(&self, url: String) -> bool {
            self.module.external(&url)
        }

        fn updater(&self, callback: Updater) {
            // HistoryProvider 只创建一次连接；Router 安装或刷新回调不重复监听事件。
            *self.updater.borrow_mut() = Some(callback);
        }
    }

    impl Drop for TransitionHistory {
        fn drop(&mut self) {
            let _ = self.module.disconnect();
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use dioxus::prelude::*;
    use dioxus::router::components::HistoryProvider;
    use std::{cell::RefCell, sync::Arc, time::Duration};

    #[test]
    fn native_history_retains_server_route_and_history_identity() {
        let inherited: Rc<dyn History> = Rc::new(
            dioxus::history::MemoryHistory::with_initial_path("/post/server-route".to_owned()),
        );
        let history = transition_history(inherited.clone());
        assert!(Rc::ptr_eq(&history, &inherited));
        assert_eq!(history.current_route(), "/post/server-route");
        history.push("/archives".to_owned());
        history.go_back();
        assert_eq!(history.current_route(), "/post/server-route");
    }

    /// 模拟浏览器适配器：push 记录意图，旧快照完成之前不发布新路由。
    struct DeferredHistory {
        displayed: RefCell<String>,
        pending: RefCell<Option<String>>,
        notify: RefCell<Option<Arc<dyn Fn() + Send + Sync>>>,
    }

    impl History for DeferredHistory {
        fn current_route(&self) -> String {
            self.displayed.borrow().clone()
        }

        fn push(&self, route: String) {
            *self.pending.borrow_mut() = Some(route);
        }

        fn replace(&self, route: String) {
            self.push(route);
        }

        fn go_back(&self) {}
        fn go_forward(&self) {}

        fn updater(&self, callback: Arc<dyn Fn() + Send + Sync>) {
            *self.notify.borrow_mut() = Some(callback);
        }
    }

    #[derive(Clone)]
    struct ProbeState {
        history: Rc<DeferredHistory>,
        resources: Rc<RefCell<Vec<String>>>,
        effects: Rc<RefCell<Vec<String>>>,
        scope: Rc<RefCell<Option<ScopeId>>>,
    }

    #[derive(Clone, Routable, PartialEq)]
    enum ProbeRoute {
        #[route("/preview/:slug")]
        Probe { slug: String },
    }

    #[component]
    fn Probe(slug: String) -> Element {
        let state: ProbeState = use_context();
        use_hook(|| *state.scope.borrow_mut() = Some(dioxus::dioxus_core::current_scope_id()));
        let router = dioxus::router::router();
        let committed_slug = use_memo(move || {
            let ProbeRoute::Probe { slug } = router.current::<ProbeRoute>();
            slug
        });
        let _request = use_resource(move || {
            let slug = committed_slug();
            state.resources.borrow_mut().push(slug.clone());
            async move { slug }
        });
        use_effect(move || state.effects.borrow_mut().push(committed_slug()));
        rsx! { div { "{slug}" } }
    }

    fn probe_app(state: ProbeState) -> Element {
        use_context_provider(|| state.clone());
        rsx! {
            HistoryProvider {
                history: move |_| state.history.clone() as Rc<dyn History>,
                Router::<ProbeRoute> {}
            }
        }
    }

    async fn settle(dom: &mut VirtualDom) {
        for _ in 0..10 {
            dom.render_immediate(&mut dioxus::dioxus_core::NoOpMutations);
            tokio::select! {
                biased;
                _ = dom.wait_for_work() => {},
                _ = tokio::time::sleep(Duration::from_millis(5)) => return,
            }
        }
        panic!("navigation did not settle");
    }

    #[tokio::test]
    async fn deferred_router_updates_only_reload_data_when_the_displayed_slug_changes() {
        let state = ProbeState {
            history: Rc::new(DeferredHistory {
                displayed: RefCell::new("/preview/first".to_owned()),
                pending: RefCell::new(None),
                notify: RefCell::new(None),
            }),
            resources: Rc::new(RefCell::new(Vec::new())),
            effects: Rc::new(RefCell::new(Vec::new())),
            scope: Rc::new(RefCell::new(None)),
        };
        let mut dom = VirtualDom::new_with_props(probe_app, state.clone());
        dom.rebuild_in_place();
        settle(&mut dom).await;
        assert_eq!(*state.resources.borrow(), ["first"]);
        assert_eq!(*state.effects.borrow(), ["first"]);

        // This invokes the real Router::push path, including its eager subscriber notification.
        dom.in_scope(state.scope.borrow().unwrap(), || {
            dioxus::router::navigator().push(ProbeRoute::Probe {
                slug: "second".to_owned(),
            });
        });
        settle(&mut dom).await;
        assert_eq!(state.history.current_route(), "/preview/first");
        assert_eq!(*state.resources.borrow(), ["first"]);
        assert_eq!(*state.effects.borrow(), ["first"]);

        // The browser's native update callback now publishes the actual destination.
        *state.history.displayed.borrow_mut() = state.history.pending.borrow_mut().take().unwrap();
        state.history.notify.borrow().as_ref().unwrap()();
        settle(&mut dom).await;
        assert_eq!(*state.resources.borrow(), ["first", "second"]);
        assert_eq!(*state.effects.borrow(), ["first", "second"]);
    }
}
