#![feature(test)]

extern crate test;

#[cfg(test)]
mod tests {
    use {
        super::*,
        anyhow::{anyhow, Context, Error, Result},
        async_trait::async_trait,
        flate2::read::GzDecoder,
        http_types::{HttpError, Method, RequestParam, RequestResult, Response},
        redis_types::{RedisParameter, RedisResult},
        std::{
            env,
            fs::{self, OpenOptions},
            hint, io,
            ops::Deref,
            os::unix::fs::OpenOptionsExt,
            path::Path,
            process::Command,
            sync::Once,
        },
        tar::Archive,
        test::Bencher,
        tokio::runtime::Runtime,
        wasmtime::{
            component::{Component, Instance as ComponentInstance, Linker as ComponentLinker},
            Config, Engine, InstanceAllocationStrategy, Linker as ModuleLinker, Module,
            PoolingAllocationConfig, Store,
        },
    };

    wasmtime::component::bindgen!({
        path: "wit",
        world: "spin-http",
        async: true
    });

    struct Host {
        wasi: wasi_preview2::WasiCtx,
    }

    #[async_trait]
    impl config::Host for Host {
        async fn get_config(&mut self, _key: String) -> Result<Result<String, config::Error>> {
            unreachable!()
        }
    }

    #[async_trait]
    impl redis::Host for Host {
        async fn publish(
            &mut self,
            _address: String,
            _channel: String,
            _payload: Vec<u8>,
        ) -> Result<Result<(), redis_types::Error>> {
            unreachable!()
        }

        async fn get(
            &mut self,
            _address: String,
            _key: String,
        ) -> Result<Result<Vec<u8>, redis_types::Error>> {
            unreachable!()
        }

        async fn set(
            &mut self,
            _address: String,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<Result<(), redis_types::Error>> {
            unreachable!()
        }

        async fn incr(
            &mut self,
            _address: String,
            _key: String,
        ) -> Result<Result<i64, redis_types::Error>> {
            unreachable!()
        }

        async fn del(
            &mut self,
            _address: String,
            _keys: Vec<String>,
        ) -> Result<Result<i64, redis_types::Error>> {
            unreachable!()
        }

        async fn sadd(
            &mut self,
            _address: String,
            _key: String,
            _values: Vec<String>,
        ) -> Result<Result<i64, redis_types::Error>> {
            unreachable!()
        }

        async fn smembers(
            &mut self,
            _address: String,
            _key: String,
        ) -> Result<Result<Vec<String>, redis_types::Error>> {
            unreachable!()
        }

        async fn srem(
            &mut self,
            _address: String,
            _key: String,
            _values: Vec<String>,
        ) -> Result<Result<i64, redis_types::Error>> {
            unreachable!()
        }

        async fn execute(
            &mut self,
            _address: String,
            _command: String,
            _arguments: Vec<RedisParameter>,
        ) -> Result<Result<Vec<RedisResult>, redis_types::Error>> {
            unreachable!()
        }
    }

    #[async_trait]
    impl key_value::Host for Host {
        async fn open(
            &mut self,
            _name: String,
        ) -> Result<Result<key_value::Store, key_value::Error>> {
            unreachable!()
        }

        async fn get(
            &mut self,
            _store: key_value::Store,
            _key: String,
        ) -> Result<Result<Vec<u8>, key_value::Error>> {
            unreachable!()
        }

        async fn set(
            &mut self,
            _store: key_value::Store,
            _key: String,
            _value: Vec<u8>,
        ) -> Result<Result<(), key_value::Error>> {
            unreachable!()
        }

        async fn delete(
            &mut self,
            _store: key_value::Store,
            _key: String,
        ) -> Result<Result<(), key_value::Error>> {
            unreachable!()
        }

        async fn exists(
            &mut self,
            _store: key_value::Store,
            _key: String,
        ) -> Result<Result<bool, key_value::Error>> {
            unreachable!()
        }

        async fn get_keys(
            &mut self,
            _store: key_value::Store,
        ) -> Result<Result<Vec<String>, key_value::Error>> {
            unreachable!()
        }

        async fn close(&mut self, _store: key_value::Store) -> Result<()> {
            unreachable!()
        }
    }

    #[async_trait]
    impl http::Host for Host {
        async fn send_request(
            &mut self,
            _req: RequestResult,
        ) -> Result<Result<Response, HttpError>> {
            unreachable!()
        }
    }

    fn add_to_linker(linker: &mut ComponentLinker<Host>) -> anyhow::Result<()> {
        wasi_host::command::add_to_linker(linker, |host| &mut host.wasi)?;
        config::add_to_linker(linker, |host| host)?;
        redis::add_to_linker(linker, |host| host)?;
        key_value::add_to_linker(linker, |host| host)?;
        http::add_to_linker(linker, |host| host)?;

        Ok(())
    }

    fn build_python_app() -> Result<()> {
        let out_dir = Path::new(env!("OUT_DIR"));
        let wasm_path = out_dir.join("python-spin-guest.wasm");
        let py2wasm_path = out_dir.join("py2wasm");
        if !py2wasm_path.exists() {
            let tar = Runtime::new()?.block_on(async {
                reqwest::get(&format!(
                    "https://github.com/fermyon/spin-python-sdk/\
                     releases/download/v0.1.1/py2wasm-v0.1.1-{}-{}.tar.gz",
                    env::consts::OS,
                    env::consts::ARCH
                ))
                .await?
                .error_for_status()?
                .bytes()
                .await
            })?;

            let mut tar = Archive::new(GzDecoder::new(tar.deref()));

            for file in tar.entries()? {
                let mut file = file?;
                if let Some("py2wasm") = file.path()?.to_str() {
                    io::copy(
                        &mut file,
                        &mut OpenOptions::new()
                            .write(true)
                            .create(true)
                            .mode(0o744)
                            .open(&py2wasm_path)?,
                    )?;
                    break;
                }
            }
        }

        assert!(Command::new(py2wasm_path)
            .current_dir("python-spin-guest")
            .arg("app")
            .arg("-o")
            .arg(wasm_path)
            .status()?
            .success());

        Ok(())
    }

    fn compile_guests() {
        static ONCE: Once = Once::new();

        let once = || {
            for guest in ["wagi-guest", "spin-guest", "spin-sdk-guest"] {
                let mut cmd = Command::new("cargo");
                cmd.arg("build")
                    .current_dir(guest)
                    .arg("--release")
                    .arg("--target=wasm32-wasi")
                    .env("CARGO_TARGET_DIR", env!("OUT_DIR"));

                let status = cmd.status()?;
                assert!(status.success());
            }

            build_python_app()?;

            Ok::<(), Error>(())
        };

        ONCE.call_once(|| once().unwrap())
    }

    enum Mode {
        Direct,
        Fork,
    }

    fn bench(bencher: &mut Bencher, test: impl Fn(), mode: Mode) {
        match mode {
            Mode::Direct => bencher.iter(test),
            Mode::Fork => bencher.iter(do_fork(test)),
        }
    }

    fn do_fork(fun: impl Fn()) -> impl Fn() {
        move || {
            match unsafe { libc::fork() } {
                -1 => panic!("fork failed; errno: {}", errno::errno()),
                0 => {
                    // I'm the child
                    fun();

                    // Exit without running any destructors for maximum performance
                    unsafe { libc::_exit(0) }
                }
                child => {
                    // I'm the parent
                    let mut status = 0;
                    if -1 == unsafe { libc::waitpid(child, &mut status, 0) } {
                        panic!("waitpid failed; errno: {}", errno::errno());
                    }

                    if !(libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0) {
                        panic!(
                            "child exited{}",
                            if libc::WIFEXITED(status) {
                                format!(" (exit status {})", libc::WEXITSTATUS(status))
                            } else if libc::WIFSIGNALED(status) {
                                format!(" (killed by signal {})", libc::WTERMSIG(status))
                            } else {
                                String::new()
                            }
                        )
                    }
                }
            }
        }
    }

    fn spin_native_response(bencher: &mut Bencher, mode: Mode) -> Result<()> {
        // not actually needed here, but it makes the output more readable to get the compilation step done before
        // any benchmarks run:
        compile_guests();

        let handle_request =
            <spin_guest::InboundHttp as spin_guest::inbound_http::InboundHttp>::handle_request;

        bench(
            bencher,
            || {
                let response = hint::black_box(handle_request)(spin_guest::RequestResult {
                    method: spin_guest::Method::Post,
                    uri: "/foo?a=b".to_owned(),
                    headers: vec![("what".to_owned(), "up".to_owned())],
                    params: Vec::new(),
                    body: Some(b"hello, world!".to_vec()),
                });

                let headers = response.headers.unwrap();
                assert_eq!(1, headers.len());
                assert_eq!("content-type", headers[0].0);
                assert_eq!("text/plain", headers[0].1);
                assert_eq!(Some(b"hola, mundo!" as &[_]), response.body.as_deref());
            },
            mode,
        );

        Ok(())
    }

    async fn spin_test_instance(
        store: &mut Store<Host>,
        instance: ComponentInstance,
    ) -> Result<()> {
        let func = instance
            .exports(&mut *store)
            .instance("inbound-http")
            .ok_or_else(|| anyhow!("no inbound-http instance found"))?
            .typed_func::<(RequestParam,), (Response,)>("handle-request")?;

        let (response,) = func
            .call_async(
                store,
                (RequestParam {
                    method: Method::Post,
                    uri: "/foo?a=b",
                    headers: &[("what", "up")],
                    params: &[],
                    body: Some(b"hello, world!"),
                },),
            )
            .await?;

        eprintln!(
            "got response: {response:?} ({:?})",
            response.body.as_deref().map(|v| String::from_utf8_lossy(v))
        );

        let headers = response.headers.unwrap();
        assert_eq!(1, headers.len());
        assert_eq!("content-type", headers[0].0);
        assert_eq!("text/plain", headers[0].1);
        assert_eq!(Some(b"hola, mundo!" as &[_]), response.body.as_deref());

        Ok(())
    }

    fn spin_response(bencher: &mut Bencher, wasm_path: &str, mut config: Config) -> Result<()> {
        compile_guests();

        config.async_support(true);
        config.wasm_component_model(true);
        let engine = &Engine::new(&config)?;
        let mut linker = ComponentLinker::new(engine);
        add_to_linker(&mut linker)?;
        let pre = linker.instantiate_pre(&Component::new(
            engine,
            spin_componentize::componentize(&fs::read(format!("{}{wasm_path}", env!("OUT_DIR")))?)?,
        )?)?;

        let run = || async {
            let mut store = Store::new(
                engine,
                Host {
                    wasi: wasmtime_wasi_preview2::WasiCtxBuilder::new()
                        .inherit_stdout()
                        .inherit_stderr()
                        .build(),
                },
            );
            let instance = pre.instantiate_async(&mut store).await?;

            spin_test_instance(&mut store, instance).await
        };

        let runtime = Runtime::new()?;

        bencher.iter(|| runtime.block_on(run()).unwrap());

        Ok(())
    }

    #[bench]
    fn spin_native_direct_response(bencher: &mut Bencher) -> Result<()> {
        spin_native_response(bencher, Mode::Direct)
    }

    #[bench]
    fn spin_native_fork_response(bencher: &mut Bencher) -> Result<()> {
        spin_native_response(bencher, Mode::Fork)
    }

    #[bench]
    fn wagi_response(bencher: &mut Bencher) -> Result<()> {
        compile_guests();

        let mut config = Config::new();
        config.async_support(true);
        let engine = &Engine::new(&config)?;
        let mut linker = ModuleLinker::new(engine);
        wasmtime_wasi_preview1::add_to_linker(&mut linker, |ctx| ctx)?;
        let pre = linker.instantiate_pre(&Module::new(
            engine,
            fs::read(concat!(
                env!("OUT_DIR"),
                "/wasm32-wasi/release/wagi-guest.wasm"
            ))?,
        )?)?;

        let run = || async {
            let mut ctx = wasmtime_wasi_preview1::WasiCtxBuilder::new().build();
            ctx.push_arg("/foo")?;
            ctx.push_arg("a=b")?;
            ctx.push_env("what", "up")?;
            ctx.set_stdin(Box::new(wasi_preview1::pipe::ReadPipe::new(
                b"hello, world!" as &[_],
            )));
            let stdout = wasi_preview1::pipe::WritePipe::new_in_memory();
            ctx.set_stdout(Box::new(stdout.clone()));
            let stderr = wasi_preview1::pipe::WritePipe::new_in_memory();
            ctx.set_stderr(Box::new(stderr.clone()));

            let mut store = Store::new(engine, ctx);
            let instance = pre.instantiate_async(&mut store).await?;

            let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;

            let result = start.call_async(&mut store, ()).await;

            drop(store);

            result.with_context(|| {
                String::from_utf8_lossy(&stderr.try_into_inner().unwrap().into_inner()).to_string()
            })?;

            assert_eq!(
                b"content-type: text/plain\n\nhola, mundo!\n" as &[_],
                &stdout.try_into_inner().unwrap().into_inner()
            );

            Ok::<(), Error>(())
        };

        let runtime = Runtime::new()?;

        bencher.iter(|| runtime.block_on(run()).unwrap());

        Ok(())
    }

    #[bench]
    fn spin_sdk_response(bencher: &mut Bencher) -> Result<()> {
        spin_response(
            bencher,
            "/wasm32-wasi/release/spin_sdk_guest.wasm",
            Config::new(),
        )
    }

    #[bench]
    fn spin_python_response(bencher: &mut Bencher) -> Result<()> {
        let mut config = Config::new();
        config.allocation_strategy(InstanceAllocationStrategy::Pooling(
            PoolingAllocationConfig::default(),
        ));
        spin_response(bencher, "/python-spin-guest.wasm", config)
    }

    #[bench]
    fn spin_raw_response(bencher: &mut Bencher) -> Result<()> {
        spin_response(
            bencher,
            "/wasm32-wasi/release/spin_guest.wasm",
            Config::new(),
        )
    }

    #[bench]
    fn spin_raw_response_with_pooling(bencher: &mut Bencher) -> Result<()> {
        let mut config = Config::new();
        config.allocation_strategy(InstanceAllocationStrategy::Pooling(
            PoolingAllocationConfig::default(),
        ));
        spin_response(bencher, "/wasm32-wasi/release/spin_guest.wasm", config)
    }

    #[bench]
    fn spin_raw_response_no_pre_instantiation(bencher: &mut Bencher) -> Result<()> {
        compile_guests();

        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        let engine = &Engine::new(&config)?;
        let mut linker = ComponentLinker::new(engine);
        add_to_linker(&mut linker)?;
        let component = spin_componentize::componentize(&fs::read(format!(
            "{}/wasm32-wasi/release/spin_guest.wasm",
            env!("OUT_DIR")
        ))?)?;

        let run = || async {
            let mut store = Store::new(
                engine,
                Host {
                    wasi: wasmtime_wasi_preview2::WasiCtxBuilder::new().build(),
                },
            );
            let instance = linker
                .instantiate_async(&mut store, &Component::new(engine, &component)?)
                .await?;

            spin_test_instance(&mut store, instance).await
        };

        let runtime = Runtime::new()?;

        bencher.iter(|| runtime.block_on(run()).unwrap());

        Ok(())
    }

    #[bench]
    fn spin_raw_response_pre_compile(bencher: &mut Bencher) -> Result<()> {
        compile_guests();

        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        let engine = &Engine::new(&config)?;
        let mut linker = ComponentLinker::new(engine);
        add_to_linker(&mut linker)?;
        let component = spin_componentize::componentize(&fs::read(format!(
            "{}/wasm32-wasi/release/spin_guest.wasm",
            env!("OUT_DIR")
        ))?)?;
        let cwasm = engine.precompile_component(&component)?;

        let run = || async {
            let mut store = Store::new(
                engine,
                Host {
                    wasi: wasmtime_wasi_preview2::WasiCtxBuilder::new().build(),
                },
            );
            let instance = linker
                .instantiate_async(&mut store, &unsafe {
                    Component::deserialize(engine, &cwasm)
                }?)
                .await?;

            spin_test_instance(&mut store, instance).await
        };

        let runtime = Runtime::new()?;

        bencher.iter(|| runtime.block_on(run()).unwrap());

        Ok(())
    }
}
