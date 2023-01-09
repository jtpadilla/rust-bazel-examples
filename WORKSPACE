
load("@bazel_tools//tools/build_defs/repo:http.bzl", "http_archive")
http_archive(
    name = "rules_rust",
    sha256 = "aaaa4b9591a5dad8d8907ae2dbe6e0eb49e6314946ce4c7149241648e56a1277",
    urls = ["https://github.com/bazelbuild/rules_rust/releases/download/0.16.1/rules_rust-v0.16.1.tar.gz"],
)

load("@rules_rust//rust:repositories.bzl", "rules_rust_dependencies", "rust_register_toolchains")
rules_rust_dependencies()
rust_register_toolchains()

load("@rules_rust//tools/rust_analyzer:deps.bzl", "rust_analyzer_dependencies")
rust_analyzer_dependencies()

load("@rules_rust//crate_universe:repositories.bzl", "crate_universe_dependencies")
crate_universe_dependencies()

load("@rules_rust//crate_universe:defs.bzl", "crate", "crates_repository", "render_config")
crates_repository(
    name = "crate_index",
    cargo_lockfile = "//:Cargo.lock",
    lockfile = "//:Cargo.Bazel.lock",
    packages = {
        "async-trait": crate.spec(
            version = "0.1.51",
        ),
        "mockall": crate.spec(
            version = "0.10.2",
        ),
        "tokio": crate.spec(
            version = "1",
            features = ["full"]
        ),
        "bytes": crate.spec(
            version = "1",
        ),
        "async-stream": crate.spec(
            version = "0.3.0",
        ),
        "atoi": crate.spec(
            version = "0.3.2",
        ),
        "rand": crate.spec(
            version = "0.8.5",
        ),
        "clap": crate.spec(
            version = "3.1.18",
            features = ["derive"]
        ),
        "tokio-stream": crate.spec(
            version = "0.1",
        ),
        "tracing": crate.spec(
            version = "0.1.34",
        ),
        "tracing-futures" : crate.spec(
            version = "0.2.3",
        ),
        "tracing-subscriber" : crate.spec(
            version = "0.3.11", 
            features = ["env-filter"],
        ),
        # Implements the types defined in the OTel spec
        "opentelemetry": crate.spec(
            version = "0.17.0", 
        ),            
        # Integration between the tracing crate and the opentelemetry crate
        "tracing-opentelemetry" : crate.spec(
            version = "0.17.2", 
        ),
        # Provides a "propagator" to pass along an XrayId across services
        "opentelemetry-aws" : crate.spec(
            version = "0.5.0", 
        ),
        # Allows you to send data to the OTel collector
        "opentelemetry-otlp" : crate.spec(
            version = "0.10.0", 
        ),
    },
    # Setting the default package name to `""` forces the use of the macros defined in this repository
    # to always use the root package when looking for dependencies or aliases. This should be considered
    # optional as the repository also exposes alises for easy access to all dependencies.
    render_config = render_config(
        default_package_name = ""
    ),
  
)

load("@crate_index//:defs.bzl", "crate_repositories")
crate_repositories()

load("@rules_rust//proto:repositories.bzl", "rust_proto_repositories")
rust_proto_repositories()

load("@rules_rust//proto:transitive_repositories.bzl", "rust_proto_transitive_repositories")
rust_proto_transitive_repositories()

