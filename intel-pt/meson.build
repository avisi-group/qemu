intel_pt_ss.add(files('config.c'))
intel_pt_ss.add(files('recording.c'))
intel_pt_ss.add(files('arguments.c'))
intel_pt_ss.add(files('mapping.c'))
intel_pt_ss.add(files('cleanup.c'))
intel_pt_ss.add(files('chain-count.c'))
intel_pt_ss.add(files('jmx-jump.c'))

intel_pt_parser_ss = ss.source_set()

subdir('parser')

intel_pt_ss.add_all(intel_pt_parser_ss)