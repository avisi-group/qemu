find_program('cargo', required : true)

cargo_build = find_program('cargo.sh')

env = environment()
env.set('CARGO_TARGET_DIR', join_paths(meson.current_build_dir(), 'target'))

message('Running cargo...')

cargo_result = run_command(cargo_build, env : env, check: true)
cargo_stdout = cargo_result.stdout().strip()
cargo_stderr = cargo_result.stderr().strip()
if cargo_stderr != ''
    message(cargo_stderr)
elif cargo_stdout != ''
    message(cargo_stdout)
else
    message('Cargo is done')
endif

# Here we grab any shared library objects from Cargo's generated "target" directory
# using a shell script to take advantage of wildcards.
get_objects = find_program('get_objects.sh')

# The output is sanitized and made into an array.
cargo_objects = run_command(get_objects, env : env, check: true).stdout().strip().split('\n')

message('Found objects:')
message(cargo_objects)

scribe_header = files('bindings.h')

intel_pt = declare_dependency(
  include_directories : include_directories('.'),
  link_args : cargo_objects
)

subdir('csrc')
