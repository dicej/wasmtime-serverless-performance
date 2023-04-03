#![feature(test)]

extern crate test;

#[cfg(test)]
mod tests {
    use {
        super::*,
        anyhow::{anyhow, Context, Error, Result},
        http_types::{Method, Request, Response},
        std::{fs, process::Command, sync::Once},
        test::Bencher,
        tokio::runtime::Runtime,
        wasmtime::{
            component::{Component, Linker as ComponentLinker},
            Config, Engine, Linker as ModuleLinker, Module, Store,
        },
    };

    wasmtime::component::bindgen!({
        path: "wit",
        world: "spin-http",
        async: true
    });

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

                let status = cmd.status().unwrap();
                assert!(status.success());
            }

            Ok::<(), Error>(())
        };

        ONCE.call_once(|| once().unwrap())
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
        spin_response(bencher, "/wasm32-wasi/release/spin_sdk_guest.wasm")
    }

    #[bench]
    fn spin_raw_response(bencher: &mut Bencher) -> Result<()> {
        spin_response(bencher, "/wasm32-wasi/release/spin_guest.wasm")
    }

    fn spin_response(bencher: &mut Bencher, wasm_path: &str) -> Result<()> {
        compile_guests();

        let mut config = Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        let engine = &Engine::new(&config)?;
        let mut linker = ComponentLinker::new(engine);
        wasi_host::command::add_to_linker(&mut linker, |ctx| ctx)?;
        let pre = linker.instantiate_pre(&Component::new(
            engine,
            spin_componentize::componentize(&fs::read(&format!(
                "{}{wasm_path}",
                env!("OUT_DIR")
            ))?)?,
        )?)?;

        let run = || async {
            let ctx = wasmtime_wasi_preview2::WasiCtxBuilder::new().build();
            let mut store = Store::new(engine, ctx);
            let instance = pre.instantiate_async(&mut store).await?;

            let func = instance
                .exports(&mut store)
                .instance("inbound-http")
                .ok_or_else(|| anyhow!("no inbound-http instance found"))?
                .typed_func::<(Request,), (Response,)>("handle-request")?;

            let (response,) = func
                .call_async(
                    store,
                    (Request {
                        method: Method::Post,
                        uri: "/foo?a=b",
                        headers: &[("what", "up")],
                        params: &[],
                        body: Some(b"hello, world!"),
                    },),
                )
                .await?;

            assert_eq!(
                Some(&[("content-type".to_owned(), "text/plain".to_owned())] as &[_]),
                response.headers.as_deref()
            );
            assert_eq!(Some(b"hola, mundo!" as &[_]), response.body.as_deref());

            Ok::<(), Error>(())
        };

        let runtime = Runtime::new()?;

        bencher.iter(|| runtime.block_on(run()).unwrap());

        Ok(())
    }
}
