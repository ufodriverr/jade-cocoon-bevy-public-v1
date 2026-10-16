# Third-party notices

This release uses the Cargo.lock dependency versions listed below. Their licences
apply to their own code and other bundled material, separately from GPL-3.0-only
for the original project work. Full available notices are retained in `licenses/`.
The matching complete source archive also retains the upstream crate contents.
This inventory includes platform-specific and build/test dependencies, so not every
listed package is linked into the Windows executable. Cargo licence expressions
are reproduced as declared by upstream, including legacy slash notation. Where
a package offers alternative licences, a GPLv3-compatible alternative applies;
for MIT/Apache-2.0 alternatives, either is compatible. All required combined
licences and additional notices remain applicable. No LGPL alternative is selected
for r-efi; its MIT alternative is available. Bundled subcomponents can have their
own notices in the preserved upstream source and notice directories. Some crates
omit a standalone licence file; their published licence declaration and standard
licence text or notices from the same upstream project are retained. Notice origins
and these supplements are recorded in `licenses/NOTICE-ORIGINS.json`.

Version 1.1 adds the networking of Arena Battle (iroh, tokio, rustls and related
crates). Its TLS uses the `ring` crypto provider (`Apache-2.0 AND ISC`); code that
`ring` took from BoringSSL is under Apache-2.0, as its `LICENSE-BoringSSL` and file
headers state. `webpki-roots` and `webpki-root-certs` carry Mozilla's root certificate
data under the Community Data License Agreement Permissive 2.0, whose text is kept
with them. `attohttpc` (MPL-2.0) is used for router port forwarding; its source is in
the complete source archive.

| Package | Version | Declared licence | Notices |
|---|---|---|---|
| ab_glyph | 0.2.32 | Apache-2.0 | [Files](licenses/ab_glyph-0.2.32) |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | [Files](licenses/ab_glyph_rasterizer-0.1.10) |
| accesskit | 0.24.1 | MIT OR Apache-2.0 | [Files](licenses/accesskit-0.24.1) |
| accesskit_consumer | 0.35.0 | MIT OR Apache-2.0 | [Files](licenses/accesskit_consumer-0.35.0) |
| accesskit_consumer | 0.38.0 | MIT OR Apache-2.0 | [Files](licenses/accesskit_consumer-0.38.0) |
| accesskit_macos | 0.26.3 | MIT OR Apache-2.0 | [Files](licenses/accesskit_macos-0.26.3) |
| accesskit_windows | 0.32.1 | MIT OR Apache-2.0 | [Files](licenses/accesskit_windows-0.32.1) |
| accesskit_winit | 0.32.2 | Apache-2.0 | [Files](licenses/accesskit_winit-0.32.2) |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | [Files](licenses/adler2-2.0.1) |
| aead | 0.5.2 | MIT OR Apache-2.0 | [Files](licenses/aead-0.5.2) |
| aes | 0.8.4 | MIT OR Apache-2.0 | [Files](licenses/aes-0.8.4) |
| aes-gcm | 0.10.3 | Apache-2.0 OR MIT | [Files](licenses/aes-gcm-0.10.3) |
| ahash | 0.8.12 | MIT OR Apache-2.0 | [Files](licenses/ahash-0.8.12) |
| aho-corasick | 1.1.5 | Unlicense OR MIT | [Files](licenses/aho-corasick-1.1.5) |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | [Files](licenses/allocator-api2-0.2.21) |
| alsa | 0.11.0 | Apache-2.0/MIT | [Files](licenses/alsa-0.11.0) |
| alsa-sys | 0.4.0 | MIT | [Files](licenses/alsa-sys-0.4.0) |
| android-activity | 0.6.1 | MIT OR Apache-2.0 | [Files](licenses/android-activity-0.6.1) |
| android-properties | 0.2.2 | MIT | [Files](licenses/android-properties-0.2.2) |
| android_log-sys | 0.3.2 | MIT OR Apache-2.0 | [Files](licenses/android_log-sys-0.3.2) |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | [Files](licenses/android_system_properties-0.1.6) |
| approx | 0.5.1 | Apache-2.0 | [Files](licenses/approx-0.5.1) |
| arboard | 3.6.1 | MIT OR Apache-2.0 | [Files](licenses/arboard-3.6.1) |
| arc-swap | 1.9.2 | MIT OR Apache-2.0 | [Files](licenses/arc-swap-1.9.2) |
| arrayref | 0.3.9 | BSD-2-Clause | [Files](licenses/arrayref-0.3.9) |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | [Files](licenses/arrayvec-0.7.8) |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | [Files](licenses/as-raw-xcb-connection-1.0.1) |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | [Files](licenses/ash-0.38.0+1.3.281) |
| assert_type_match | 0.1.1 | MIT OR Apache-2.0 | [Files](licenses/assert_type_match-0.1.1) |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | [Files](licenses/async-broadcast-0.7.2) |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | [Files](licenses/async-channel-2.5.0) |
| async-executor | 1.14.0 | Apache-2.0 OR MIT | [Files](licenses/async-executor-1.14.0) |
| async-fs | 2.2.0 | Apache-2.0 OR MIT | [Files](licenses/async-fs-2.2.0) |
| async-io | 2.6.0 | Apache-2.0 OR MIT | [Files](licenses/async-io-2.6.0) |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | [Files](licenses/async-lock-3.4.2) |
| async-task | 4.7.1 | Apache-2.0 OR MIT | [Files](licenses/async-task-4.7.1) |
| async_io_stream | 0.3.3 | Unlicense | [Files](licenses/async_io_stream-0.3.3) |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | [Files](licenses/atomic-waker-1.1.2) |
| atomicow | 1.2.0 | MIT OR Apache-2.0 | [Files](licenses/atomicow-1.2.0) |
| attohttpc | 0.30.1 | MPL-2.0 | [Files](licenses/attohttpc-0.30.1) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [Files](licenses/autocfg-1.5.1) |
| backon | 1.6.0 | Apache-2.0 | [Files](licenses/backon-1.6.0) |
| base16ct | 1.0.0 | Apache-2.0 OR MIT | [Files](licenses/base16ct-1.0.0) |
| base64 | 0.22.1 | MIT OR Apache-2.0 | [Files](licenses/base64-0.22.1) |
| base64 | 0.23.1 | MIT OR Apache-2.0 | [Files](licenses/base64-0.23.1) |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | [Files](licenses/base64ct-1.8.3) |
| bevy | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy-0.19.1) |
| bevy_a11y | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_a11y-0.19.1) |
| bevy_android | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_android-0.19.1) |
| bevy_animation | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_animation-0.19.1) |
| bevy_animation_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_animation_macros-0.19.1) |
| bevy_anti_alias | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_anti_alias-0.19.1) |
| bevy_app | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_app-0.19.1) |
| bevy_asset | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_asset-0.19.1) |
| bevy_asset_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_asset_macros-0.19.1) |
| bevy_audio | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_audio-0.19.1) |
| bevy_camera | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_camera-0.19.1) |
| bevy_clipboard | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_clipboard-0.19.1) |
| bevy_color | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_color-0.19.1) |
| bevy_core_pipeline | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_core_pipeline-0.19.1) |
| bevy_derive | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_derive-0.19.1) |
| bevy_dev_tools | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_dev_tools-0.19.1) |
| bevy_diagnostic | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_diagnostic-0.19.1) |
| bevy_ecs | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ecs-0.19.1) |
| bevy_ecs_macro_logic | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ecs_macro_logic-0.19.1) |
| bevy_ecs_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ecs_macros-0.19.1) |
| bevy_encase_derive | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_encase_derive-0.19.1) |
| bevy_feathers | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_feathers-0.19.1) |
| bevy_gilrs | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_gilrs-0.19.1) |
| bevy_gizmos | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_gizmos-0.19.1) |
| bevy_gizmos_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_gizmos_macros-0.19.1) |
| bevy_gizmos_render | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_gizmos_render-0.19.1) |
| bevy_gltf | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_gltf-0.19.1) |
| bevy_image | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_image-0.19.1) |
| bevy_input | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_input-0.19.1) |
| bevy_input_focus | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_input_focus-0.19.1) |
| bevy_internal | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_internal-0.19.1) |
| bevy_light | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_light-0.19.1) |
| bevy_log | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_log-0.19.1) |
| bevy_macro_utils | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_macro_utils-0.19.1) |
| bevy_material | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_material-0.19.1) |
| bevy_material_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_material_macros-0.19.1) |
| bevy_math | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_math-0.19.1) |
| bevy_mesh | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_mesh-0.19.1) |
| bevy_mikktspace | 1.0.0 | Zlib AND (MIT OR Apache-2.0) | [Files](licenses/bevy_mikktspace-1.0.0) |
| bevy_pbr | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_pbr-0.19.1) |
| bevy_picking | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_picking-0.19.1) |
| bevy_platform | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_platform-0.19.1) |
| bevy_post_process | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_post_process-0.19.1) |
| bevy_ptr | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ptr-0.19.1) |
| bevy_reflect | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_reflect-0.19.1) |
| bevy_reflect_derive | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_reflect_derive-0.19.1) |
| bevy_render | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_render-0.19.1) |
| bevy_render_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_render_macros-0.19.1) |
| bevy_scene | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_scene-0.19.1) |
| bevy_scene_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_scene_macros-0.19.1) |
| bevy_shader | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_shader-0.19.1) |
| bevy_sprite | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_sprite-0.19.1) |
| bevy_sprite_render | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_sprite_render-0.19.1) |
| bevy_state | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_state-0.19.1) |
| bevy_state_macros | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_state_macros-0.19.1) |
| bevy_tasks | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_tasks-0.19.1) |
| bevy_text | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_text-0.19.1) |
| bevy_time | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_time-0.19.1) |
| bevy_transform | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_transform-0.19.1) |
| bevy_ui | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ui-0.19.1) |
| bevy_ui_render | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ui_render-0.19.1) |
| bevy_ui_widgets | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_ui_widgets-0.19.1) |
| bevy_utils | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_utils-0.19.1) |
| bevy_window | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_window-0.19.1) |
| bevy_winit | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_winit-0.19.1) |
| bevy_world_serialization | 0.19.1 | MIT OR Apache-2.0 | [Files](licenses/bevy_world_serialization-0.19.1) |
| bit-set | 0.9.1 | Apache-2.0 OR MIT | [Files](licenses/bit-set-0.9.1) |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | [Files](licenses/bit-vec-0.9.1) |
| bitflags | 1.3.2 | MIT/Apache-2.0 | [Files](licenses/bitflags-1.3.2) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [Files](licenses/bitflags-2.13.2) |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | [Files](licenses/blake3-1.8.7) |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | [Files](licenses/block-buffer-0.12.1) |
| block2 | 0.5.1 | MIT | [Files](licenses/block2-0.5.1) |
| block2 | 0.6.2 | MIT | [Files](licenses/block2-0.6.2) |
| blocking | 1.7.0 | Apache-2.0 OR MIT | [Files](licenses/blocking-1.7.0) |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | [Files](licenses/bumpalo-3.20.3) |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/bytemuck-1.25.2) |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/bytemuck_derive-1.12.1) |
| byteorder | 1.5.0 | Unlicense OR MIT | [Files](licenses/byteorder-1.5.0) |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | [Files](licenses/byteorder-lite-0.1.0) |
| bytes | 1.12.1 | MIT | [Files](licenses/bytes-1.12.1) |
| calloop | 0.13.0 | MIT | [Files](licenses/calloop-0.13.0) |
| calloop-wayland-source | 0.3.0 | MIT | [Files](licenses/calloop-wayland-source-0.3.0) |
| cc | 1.6.0 | MIT OR Apache-2.0 | [Files](licenses/cc-1.6.0) |
| cesu8 | 1.1.0 | Apache-2.0/MIT | [Files](licenses/cesu8-1.1.0) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [Files](licenses/cfg-if-1.0.5) |
| cfg_aliases | 0.2.2 | MIT | [Files](licenses/cfg_aliases-0.2.2) |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | [Files](licenses/chacha20-0.10.2) |
| chrono | 0.4.45 | MIT OR Apache-2.0 | [Files](licenses/chrono-0.4.45) |
| cipher | 0.4.4 | MIT OR Apache-2.0 | [Files](licenses/cipher-0.4.4) |
| clipboard-win | 5.4.1 | BSL-1.0 | [Files](licenses/clipboard-win-5.4.1) |
| cmov | 0.5.4 | Apache-2.0 OR MIT | [Files](licenses/cmov-0.5.4) |
| cobs | 0.3.0 | MIT OR Apache-2.0 | [Files](licenses/cobs-0.3.0) |
| codespan-reporting | 0.12.0 | Apache-2.0 | [Files](licenses/codespan-reporting-0.12.0) |
| codespan-reporting | 0.13.1 | Apache-2.0 | [Files](licenses/codespan-reporting-0.13.1) |
| combine | 4.6.8 | MIT | [Files](licenses/combine-4.6.8) |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | [Files](licenses/concurrent-queue-2.5.0) |
| console_error_panic_hook | 0.1.7 | Apache-2.0/MIT | [Files](licenses/console_error_panic_hook-0.1.7) |
| const-fnv1a-hash | 1.1.0 | MIT | [Files](licenses/const-fnv1a-hash-1.1.0) |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | [Files](licenses/const-oid-0.10.2) |
| const_panic | 0.2.17 | Zlib | [Files](licenses/const_panic-0.2.17) |
| const_soft_float | 0.1.4 | MIT OR Apache-2.0 | [Files](licenses/const_soft_float-0.1.4) |
| constant_time_eq | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 | [Files](licenses/constant_time_eq-0.4.2) |
| constgebra | 0.1.4 | MIT OR Apache-2.0 | [Files](licenses/constgebra-0.1.4) |
| convert_case | 0.10.0 | MIT | [Files](licenses/convert_case-0.10.0) |
| cordyceps | 0.3.5 | MIT | [Files](licenses/cordyceps-0.3.5) |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | [Files](licenses/core-foundation-0.9.4) |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | [Files](licenses/core-foundation-0.10.1) |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | [Files](licenses/core-foundation-sys-0.8.7) |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 | [Files](licenses/core-graphics-0.23.2) |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | [Files](licenses/core-graphics-types-0.1.3) |
| core_maths | 0.1.1 | MIT | [Files](licenses/core_maths-0.1.1) |
| coreaudio-rs | 0.14.2 | MIT/Apache-2.0 | [Files](licenses/coreaudio-rs-0.14.2) |
| cpal | 0.17.3 | Apache-2.0 | [Files](licenses/cpal-0.17.3) |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | [Files](licenses/cpufeatures-0.2.17) |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | [Files](licenses/cpufeatures-0.3.1) |
| crc32fast | 1.5.2 | MIT OR Apache-2.0 | [Files](licenses/crc32fast-1.5.2) |
| critical-section | 1.2.0 | MIT OR Apache-2.0 | [Files](licenses/critical-section-1.2.0) |
| crossbeam-channel | 0.5.17 | MIT OR Apache-2.0 | [Files](licenses/crossbeam-channel-0.5.17) |
| crossbeam-queue | 0.3.14 | MIT OR Apache-2.0 | [Files](licenses/crossbeam-queue-0.3.14) |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | [Files](licenses/crossbeam-utils-0.8.23) |
| crunchy | 0.2.4 | MIT | [Files](licenses/crunchy-0.2.4) |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | [Files](licenses/crypto-common-0.1.7) |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | [Files](licenses/crypto-common-0.2.2) |
| ctr | 0.9.2 | MIT OR Apache-2.0 | [Files](licenses/ctr-0.9.2) |
| ctrlc | 3.5.2 | MIT/Apache-2.0 | [Files](licenses/ctrlc-3.5.2) |
| ctutils | 0.4.3 | Apache-2.0 OR MIT | [Files](licenses/ctutils-0.4.3) |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | [Files](licenses/cursor-icon-1.2.0) |
| curve25519-dalek | 5.0.0 | BSD-3-Clause | [Files](licenses/curve25519-dalek-5.0.0) |
| curve25519-dalek-derive | 0.1.1 | MIT/Apache-2.0 | [Files](licenses/curve25519-dalek-derive-0.1.1) |
| dasp_sample | 0.11.0 | MIT OR Apache-2.0 | [Files](licenses/dasp_sample-0.11.0) |
| data-encoding | 2.11.1 | MIT | [Files](licenses/data-encoding-2.11.1) |
| data-encoding-macro | 0.1.21 | MIT | [Files](licenses/data-encoding-macro-0.1.21) |
| data-encoding-macro-internal | 0.1.19 | MIT | [Files](licenses/data-encoding-macro-internal-0.1.19) |
| der | 0.8.2 | Apache-2.0 OR MIT | [Files](licenses/der-0.8.2) |
| deranged | 0.5.9 | MIT OR Apache-2.0 | [Files](licenses/deranged-0.5.9) |
| derive_more | 2.1.1 | MIT | [Files](licenses/derive_more-2.1.1) |
| derive_more-impl | 2.1.1 | MIT | [Files](licenses/derive_more-impl-2.1.1) |
| diatomic-waker | 0.2.3 | MIT OR Apache-2.0 | [Files](licenses/diatomic-waker-0.2.3) |
| digest | 0.11.3 | MIT OR Apache-2.0 | [Files](licenses/digest-0.11.3) |
| dispatch | 0.2.0 | MIT | [Files](licenses/dispatch-0.2.0) |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/dispatch2-0.3.1) |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | [Files](licenses/displaydoc-0.2.7) |
| disqualified | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/disqualified-1.1.0) |
| dlib | 0.5.3 | MIT | [Files](licenses/dlib-0.5.3) |
| dlopen2 | 0.8.2 | MIT | [Files](licenses/dlopen2-0.8.2) |
| document-features | 0.2.12 | MIT OR Apache-2.0 | [Files](licenses/document-features-0.2.12) |
| downcast-rs | 1.2.1 | MIT/Apache-2.0 | [Files](licenses/downcast-rs-1.2.1) |
| downcast-rs | 2.0.2 | MIT OR Apache-2.0 | [Files](licenses/downcast-rs-2.0.2) |
| dpi | 0.1.2 | Apache-2.0 AND MIT | [Files](licenses/dpi-0.1.2) |
| ed25519 | 3.0.0 | Apache-2.0 OR MIT | [Files](licenses/ed25519-3.0.0) |
| ed25519-dalek | 3.0.0 | BSD-3-Clause | [Files](licenses/ed25519-dalek-3.0.0) |
| either | 1.19.0 | MIT OR Apache-2.0 | [Files](licenses/either-1.19.0) |
| embedded-io | 0.4.0 | MIT OR Apache-2.0 | [Files](licenses/embedded-io-0.4.0) |
| embedded-io | 0.6.1 | MIT OR Apache-2.0 | [Files](licenses/embedded-io-0.6.1) |
| encase | 0.12.1 | MIT-0 | [Files](licenses/encase-0.12.1) |
| encase_derive | 0.12.1 | MIT-0 | [Files](licenses/encase_derive-0.12.1) |
| encase_derive_impl | 0.12.1 | MIT-0 | [Files](licenses/encase_derive_impl-0.12.1) |
| enum-assoc | 1.4.1 | MIT OR Apache-2.0 | [Files](licenses/enum-assoc-1.4.1) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | [Files](licenses/equivalent-1.0.2) |
| erased-serde | 0.4.10 | MIT OR Apache-2.0 | [Files](licenses/erased-serde-0.4.10) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [Files](licenses/errno-0.3.14) |
| error-code | 3.4.0 | BSL-1.0 | [Files](licenses/error-code-3.4.0) |
| euclid | 0.22.14 | MIT OR Apache-2.0 | [Files](licenses/euclid-0.22.14) |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | [Files](licenses/event-listener-5.4.2) |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | [Files](licenses/event-listener-strategy-0.5.4) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | [Files](licenses/fastrand-2.5.0) |
| fdeflate | 0.3.7 | MIT OR Apache-2.0 | [Files](licenses/fdeflate-0.3.7) |
| fiat-crypto | 0.3.0 | MIT OR Apache-2.0 OR BSD-1-Clause | [Files](licenses/fiat-crypto-0.3.0) |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | [Files](licenses/find-msvc-tools-0.1.14) |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 | [Files](licenses/fixedbitset-0.5.7) |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | [Files](licenses/flate2-1.1.10) |
| fnv | 1.0.7 | Apache-2.0 / MIT | [Files](licenses/fnv-1.0.7) |
| foldhash | 0.1.5 | Zlib | [Files](licenses/foldhash-0.1.5) |
| foldhash | 0.2.0 | Zlib | [Files](licenses/foldhash-0.2.0) |
| font-types | 0.11.3 | MIT OR Apache-2.0 | [Files](licenses/font-types-0.11.3) |
| font-types | 0.12.6 | MIT OR Apache-2.0 | [Files](licenses/font-types-0.12.6) |
| fontique | 0.9.0 | Apache-2.0 OR MIT | [Files](licenses/fontique-0.9.0) |
| foreign-types | 0.5.0 | MIT/Apache-2.0 | [Files](licenses/foreign-types-0.5.0) |
| foreign-types-macros | 0.2.4 | MIT/Apache-2.0 | [Files](licenses/foreign-types-macros-0.2.4) |
| foreign-types-shared | 0.3.1 | MIT/Apache-2.0 | [Files](licenses/foreign-types-shared-0.3.1) |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | [Files](licenses/form_urlencoded-1.2.2) |
| futures | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-0.3.34) |
| futures-buffered | 0.2.13 | MIT | [Files](licenses/futures-buffered-0.2.13) |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-channel-0.3.34) |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-core-0.3.34) |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-executor-0.3.34) |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-io-0.3.34) |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | [Files](licenses/futures-lite-2.6.1) |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-macro-0.3.34) |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-sink-0.3.34) |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-task-0.3.34) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/futures-util-0.3.34) |
| generator | 0.8.11 | MIT/Apache-2.0 | [Files](licenses/generator-0.8.11) |
| generic-array | 0.14.7 | MIT | [Files](licenses/generic-array-0.14.7) |
| gethostname | 1.1.0 | Apache-2.0 | [Files](licenses/gethostname-1.1.0) |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | [Files](licenses/getrandom-0.2.17) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [Files](licenses/getrandom-0.3.4) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [Files](licenses/getrandom-0.4.3) |
| ghash | 0.5.1 | Apache-2.0 OR MIT | [Files](licenses/ghash-0.5.1) |
| gilrs | 0.11.2 | Apache-2.0/MIT | [Files](licenses/gilrs-0.11.2) |
| gilrs-core | 0.6.8 | Apache-2.0/MIT | [Files](licenses/gilrs-core-0.6.8) |
| gl_generator | 0.14.0 | Apache-2.0 | [Files](licenses/gl_generator-0.14.0) |
| glam | 0.32.1 | MIT OR Apache-2.0 | [Files](licenses/glam-0.32.1) |
| gloo-timers | 0.3.0 | MIT OR Apache-2.0 | [Files](licenses/gloo-timers-0.3.0) |
| glow | 0.17.0 | MIT OR Apache-2.0 OR Zlib | [Files](licenses/glow-0.17.0) |
| gltf | 1.4.1 | MIT OR Apache-2.0 | [Files](licenses/gltf-1.4.1) |
| gltf-derive | 1.4.1 | MIT OR Apache-2.0 | [Files](licenses/gltf-derive-1.4.1) |
| gltf-json | 1.4.1 | MIT OR Apache-2.0 | [Files](licenses/gltf-json-1.4.1) |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | [Files](licenses/glutin_wgl_sys-0.6.1) |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | [Files](licenses/gpu-allocator-0.28.0) |
| gpu-descriptor | 0.3.2 | MIT OR Apache-2.0 | [Files](licenses/gpu-descriptor-0.3.2) |
| gpu-descriptor-types | 0.2.0 | MIT OR Apache-2.0 | [Files](licenses/gpu-descriptor-types-0.2.0) |
| grid | 1.0.1 | MIT | [Files](licenses/grid-1.0.1) |
| guillotiere | 0.6.2 | MIT/Apache-2.0 | [Files](licenses/guillotiere-0.6.2) |
| h2 | 0.4.20 | MIT | [Files](licenses/h2-0.4.20) |
| half | 2.7.1 | MIT OR Apache-2.0 | [Files](licenses/half-2.7.1) |
| harfrust | 0.6.2 | MIT | [Files](licenses/harfrust-0.6.2) |
| hash32 | 0.3.1 | MIT OR Apache-2.0 | [Files](licenses/hash32-0.3.1) |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | [Files](licenses/hashbrown-0.15.5) |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | [Files](licenses/hashbrown-0.16.1) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [Files](licenses/hashbrown-0.17.1) |
| heapless | 0.9.3 | MIT OR Apache-2.0 | [Files](licenses/heapless-0.9.3) |
| heck | 0.5.0 | MIT OR Apache-2.0 | [Files](licenses/heck-0.5.0) |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | [Files](licenses/hermit-abi-0.5.3) |
| hexasphere | 18.0.0 | MIT OR Apache-2.0 | [Files](licenses/hexasphere-18.0.0) |
| hexf-parse | 0.2.1 | CC0-1.0 | [Files](licenses/hexf-parse-0.2.1) |
| http | 1.5.0 | MIT OR Apache-2.0 | [Files](licenses/http-1.5.0) |
| http-body | 1.1.0 | MIT | [Files](licenses/http-body-1.1.0) |
| http-body-util | 0.1.5 | MIT | [Files](licenses/http-body-util-0.1.5) |
| httparse | 1.10.1 | MIT OR Apache-2.0 | [Files](licenses/httparse-1.10.1) |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | [Files](licenses/httpdate-1.0.3) |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | [Files](licenses/hybrid-array-0.4.15) |
| hyper | 1.12.0 | MIT | [Files](licenses/hyper-1.12.0) |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | [Files](licenses/hyper-rustls-0.27.10) |
| hyper-util | 0.1.21 | MIT | [Files](licenses/hyper-util-0.1.21) |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 | [Files](licenses/iana-time-zone-0.1.65) |
| iana-time-zone-haiku | 0.1.2 | MIT OR Apache-2.0 | [Files](licenses/iana-time-zone-haiku-0.1.2) |
| icu_collections | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_collections-2.3.0) |
| icu_locale_core | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_locale_core-2.3.0) |
| icu_locale_fallback | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_locale_fallback-2.3.0) |
| icu_locale_fallback_data | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_locale_fallback_data-2.3.0) |
| icu_normalizer | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_normalizer-2.3.0) |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_normalizer_data-2.3.0) |
| icu_properties | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_properties-2.3.0) |
| icu_properties_data | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_properties_data-2.3.0) |
| icu_provider | 2.3.1 | Unicode-3.0 | [Files](licenses/icu_provider-2.3.1) |
| icu_segmenter | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_segmenter-2.3.0) |
| icu_segmenter_data | 2.3.0 | Unicode-3.0 | [Files](licenses/icu_segmenter_data-2.3.0) |
| identity-hash | 0.1.0 | Apache-2.0 OR MIT | [Files](licenses/identity-hash-0.1.0) |
| idna | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/idna-1.1.0) |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | [Files](licenses/idna_adapter-1.2.2) |
| if-addrs | 0.15.0 | MIT OR BSD-3-Clause | [Files](licenses/if-addrs-0.15.0) |
| igd-next | 0.17.1 | MIT | [Files](licenses/igd-next-0.17.1) |
| image | 0.25.10 | MIT OR Apache-2.0 | [Files](licenses/image-0.25.10) |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | [Files](licenses/indexmap-2.14.2) |
| inflections | 1.1.1 | MIT | [Files](licenses/inflections-1.1.1) |
| inotify | 0.11.5 | ISC | [Files](licenses/inotify-0.11.5) |
| inotify-sys | 0.1.8 | ISC | [Files](licenses/inotify-sys-0.1.8) |
| inout | 0.1.4 | MIT OR Apache-2.0 | [Files](licenses/inout-0.1.4) |
| inventory | 0.3.25 | MIT OR Apache-2.0 | [Files](licenses/inventory-0.3.25) |
| ipconfig | 0.3.4 | MIT/Apache-2.0 | [Files](licenses/ipconfig-0.3.4) |
| ipnet | 2.12.2 | MIT OR Apache-2.0 | [Files](licenses/ipnet-2.12.2) |
| iroh | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/iroh-1.3.0) |
| iroh-base | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/iroh-base-1.3.0) |
| iroh-dns | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/iroh-dns-1.3.0) |
| iroh-metrics | 1.0.2 | MIT OR Apache-2.0 | [Files](licenses/iroh-metrics-1.0.2) |
| iroh-metrics-derive | 1.0.1 | MIT OR Apache-2.0 | [Files](licenses/iroh-metrics-derive-1.0.1) |
| iroh-relay | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/iroh-relay-1.3.0) |
| itertools | 0.14.0 | MIT OR Apache-2.0 | [Files](licenses/itertools-0.14.0) |
| itoa | 1.0.18 | MIT OR Apache-2.0 | [Files](licenses/itoa-1.0.18) |
| jni | 0.21.1 | MIT/Apache-2.0 | [Files](licenses/jni-0.21.1) |
| jni | 0.22.4 | MIT OR Apache-2.0 | [Files](licenses/jni-0.22.4) |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | [Files](licenses/jni-macros-0.22.4) |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | [Files](licenses/jni-sys-0.3.1) |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | [Files](licenses/jni-sys-0.4.1) |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | [Files](licenses/jni-sys-macros-0.4.1) |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | [Files](licenses/jobserver-0.1.35) |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | [Files](licenses/js-sys-0.3.106) |
| khronos-egl | 6.0.0 | MIT/Apache-2.0 | [Files](licenses/khronos-egl-6.0.0) |
| khronos_api | 3.1.0 | Apache-2.0 | [Files](licenses/khronos_api-3.1.0) |
| ktx2 | 0.5.0 | Apache-2.0 | [Files](licenses/ktx2-0.5.0) |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | [Files](licenses/lazy_static-1.5.1) |
| lewton | 0.10.2 | MIT OR Apache-2.0 | [Files](licenses/lewton-0.10.2) |
| libc | 0.2.190 | MIT OR Apache-2.0 | [Files](licenses/libc-0.2.190) |
| libloading | 0.8.9 | ISC | [Files](licenses/libloading-0.8.9) |
| libm | 0.2.16 | MIT | [Files](licenses/libm-0.2.16) |
| libredox | 0.1.25 | MIT | [Files](licenses/libredox-0.1.25) |
| libudev-sys | 0.1.4 | MIT | [Files](licenses/libudev-sys-0.1.4) |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | [Files](licenses/linebender_resource_handle-0.1.1) |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/linux-raw-sys-0.4.15) |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/linux-raw-sys-0.12.1) |
| litemap | 0.8.3 | Unicode-3.0 | [Files](licenses/litemap-0.8.3) |
| litrs | 1.0.0 | MIT OR Apache-2.0 | [Files](licenses/litrs-1.0.0) |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | [Files](licenses/lock_api-0.4.14) |
| log | 0.4.34 | MIT OR Apache-2.0 | [Files](licenses/log-0.4.34) |
| loom | 0.7.2 | MIT | [Files](licenses/loom-0.7.2) |
| lru | 0.18.5 | MIT | [Files](licenses/lru-0.18.5) |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib | [Files](licenses/lru-slab-0.1.3) |
| mac-addr | 0.3.0 | MIT | [Files](licenses/mac-addr-0.3.0) |
| mach2 | 0.5.0 | BSD-2-Clause OR MIT OR Apache-2.0 | [Files](licenses/mach2-0.5.0) |
| matchers | 0.2.0 | MIT | [Files](licenses/matchers-0.2.0) |
| memchr | 2.8.3 | Unlicense OR MIT | [Files](licenses/memchr-2.8.3) |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | [Files](licenses/memmap2-0.9.11) |
| miniz_oxide | 0.8.9 | MIT OR Zlib OR Apache-2.0 | [Files](licenses/miniz_oxide-0.8.9) |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | [Files](licenses/miniz_oxide-0.9.1) |
| mio | 1.2.4 | MIT | [Files](licenses/mio-1.2.4) |
| moxcms | 0.8.1 | BSD-3-Clause OR Apache-2.0 | [Files](licenses/moxcms-0.8.1) |
| n0-dns-resolver | 0.1.0 | MIT OR Apache-2.0 | [Files](licenses/n0-dns-resolver-0.1.0) |
| n0-error | 1.0.1 | MIT OR Apache-2.0 | [Files](licenses/n0-error-1.0.1) |
| n0-error-macros | 1.0.1 | MIT OR Apache-2.0 | [Files](licenses/n0-error-macros-1.0.1) |
| n0-future | 0.3.2 | MIT OR Apache-2.0 | [Files](licenses/n0-future-0.3.2) |
| n0-watcher | 1.0.0 | MIT OR Apache-2.0 | [Files](licenses/n0-watcher-1.0.0) |
| naga | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/naga-29.0.4) |
| naga_oil | 0.22.0 | MIT OR Apache-2.0 | [Files](licenses/naga_oil-0.22.0) |
| ndk | 0.9.0 | MIT OR Apache-2.0 | [Files](licenses/ndk-0.9.0) |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | [Files](licenses/ndk-context-0.1.1) |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | [Files](licenses/ndk-sys-0.6.0+11769913) |
| netdev | 0.45.1 | MIT | [Files](licenses/netdev-0.45.1) |
| netdev | 0.46.3 | MIT | [Files](licenses/netdev-0.46.3) |
| netlink-packet-core | 0.8.2 | MIT | [Files](licenses/netlink-packet-core-0.8.2) |
| netlink-packet-core | 0.9.0 | MIT | [Files](licenses/netlink-packet-core-0.9.0) |
| netlink-packet-route | 0.31.0 | MIT | [Files](licenses/netlink-packet-route-0.31.0) |
| netlink-packet-route | 0.33.0 | MIT | [Files](licenses/netlink-packet-route-0.33.0) |
| netlink-proto | 0.12.2 | MIT | [Files](licenses/netlink-proto-0.12.2) |
| netlink-sys | 0.8.8 | MIT | [Files](licenses/netlink-sys-0.8.8) |
| netlink-sys | 0.9.0 | MIT | [Files](licenses/netlink-sys-0.9.0) |
| netwatch | 0.19.3 | MIT OR Apache-2.0 | [Files](licenses/netwatch-0.19.3) |
| nix | 0.31.3 | MIT | [Files](licenses/nix-0.31.3) |
| nom | 8.0.0 | MIT | [Files](licenses/nom-8.0.0) |
| nonmax | 0.5.5 | MIT OR Apache-2.0 | [Files](licenses/nonmax-0.5.5) |
| noq | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/noq-1.3.0) |
| noq-proto | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/noq-proto-1.3.0) |
| noq-udp | 1.3.0 | MIT OR Apache-2.0 | [Files](licenses/noq-udp-1.3.0) |
| ntapi | 0.4.3 | Apache-2.0 OR MIT | [Files](licenses/ntapi-0.4.3) |
| nu-ansi-term | 0.50.3 | MIT | [Files](licenses/nu-ansi-term-0.50.3) |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | [Files](licenses/num-bigint-0.4.8) |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | [Files](licenses/num-conv-0.2.2) |
| num-derive | 0.4.2 | MIT OR Apache-2.0 | [Files](licenses/num-derive-0.4.2) |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | [Files](licenses/num-integer-0.1.47) |
| num-rational | 0.4.2 | MIT OR Apache-2.0 | [Files](licenses/num-rational-0.4.2) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [Files](licenses/num-traits-0.2.19) |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [Files](licenses/num_enum-0.7.6) |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [Files](licenses/num_enum_derive-0.7.6) |
| objc-sys | 0.3.5 | MIT | [Files](licenses/objc-sys-0.3.5) |
| objc2 | 0.5.3 | MIT | [Files](licenses/objc2-0.5.3) |
| objc2 | 0.6.5 | MIT | [Files](licenses/objc2-0.6.5) |
| objc2-app-kit | 0.2.2 | MIT | [Files](licenses/objc2-app-kit-0.2.2) |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-app-kit-0.3.2) |
| objc2-audio-toolbox | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-audio-toolbox-0.3.2) |
| objc2-avf-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-avf-audio-0.3.2) |
| objc2-cloud-kit | 0.2.2 | MIT | [Files](licenses/objc2-cloud-kit-0.2.2) |
| objc2-contacts | 0.2.2 | MIT | [Files](licenses/objc2-contacts-0.2.2) |
| objc2-core-audio | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-core-audio-0.3.2) |
| objc2-core-audio-types | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-core-audio-types-0.3.2) |
| objc2-core-data | 0.2.2 | MIT | [Files](licenses/objc2-core-data-0.2.2) |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-core-foundation-0.3.2) |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-core-graphics-0.3.2) |
| objc2-core-image | 0.2.2 | MIT | [Files](licenses/objc2-core-image-0.2.2) |
| objc2-core-location | 0.2.2 | MIT | [Files](licenses/objc2-core-location-0.2.2) |
| objc2-core-wlan | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-core-wlan-0.3.2) |
| objc2-encode | 4.1.0 | MIT | [Files](licenses/objc2-encode-4.1.0) |
| objc2-foundation | 0.2.2 | MIT | [Files](licenses/objc2-foundation-0.2.2) |
| objc2-foundation | 0.3.2 | MIT | [Files](licenses/objc2-foundation-0.3.2) |
| objc2-io-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-io-kit-0.3.2) |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-io-surface-0.3.2) |
| objc2-link-presentation | 0.2.2 | MIT | [Files](licenses/objc2-link-presentation-0.2.2) |
| objc2-metal | 0.2.2 | MIT | [Files](licenses/objc2-metal-0.2.2) |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-metal-0.3.2) |
| objc2-quartz-core | 0.2.2 | MIT | [Files](licenses/objc2-quartz-core-0.2.2) |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-quartz-core-0.3.2) |
| objc2-security | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-security-0.3.2) |
| objc2-security-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-security-foundation-0.3.2) |
| objc2-symbols | 0.2.2 | MIT | [Files](licenses/objc2-symbols-0.2.2) |
| objc2-system-configuration | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/objc2-system-configuration-0.3.2) |
| objc2-ui-kit | 0.2.2 | MIT | [Files](licenses/objc2-ui-kit-0.2.2) |
| objc2-uniform-type-identifiers | 0.2.2 | MIT | [Files](licenses/objc2-uniform-type-identifiers-0.2.2) |
| objc2-user-notifications | 0.2.2 | MIT | [Files](licenses/objc2-user-notifications-0.2.2) |
| offset-allocator | 0.2.0 | MIT | [Files](licenses/offset-allocator-0.2.0) |
| ogg | 0.8.0 | BSD-3-Clause | [Files](licenses/ogg-0.8.0) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [Files](licenses/once_cell-1.21.4) |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | [Files](licenses/opaque-debug-0.3.1) |
| openssl-probe | 0.2.1 | MIT OR Apache-2.0 | [Files](licenses/openssl-probe-0.2.1) |
| orbclient | 0.3.55 | MIT | [Files](licenses/orbclient-0.3.55) |
| ordered-float | 5.5.0 | MIT | [Files](licenses/ordered-float-5.5.0) |
| os_pipe | 1.2.3 | MIT | [Files](licenses/os_pipe-1.2.3) |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | [Files](licenses/owned_ttf_parser-0.25.1) |
| papaya | 0.2.5 | MIT | [Files](licenses/papaya-0.2.5) |
| parking | 2.2.1 | Apache-2.0 OR MIT | [Files](licenses/parking-2.2.1) |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | [Files](licenses/parking_lot-0.12.5) |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | [Files](licenses/parking_lot_core-0.9.12) |
| parlance | 0.1.0 | Apache-2.0 OR MIT | [Files](licenses/parlance-0.1.0) |
| parley | 0.9.0 | Apache-2.0 OR MIT | [Files](licenses/parley-0.9.0) |
| parley_data | 0.9.0 | Apache-2.0 OR MIT | [Files](licenses/parley_data-0.9.0) |
| paste | 1.0.15 | MIT OR Apache-2.0 | [Files](licenses/paste-1.0.15) |
| pem-rfc7468 | 1.0.0 | Apache-2.0 OR MIT | [Files](licenses/pem-rfc7468-1.0.0) |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | [Files](licenses/percent-encoding-2.3.2) |
| petgraph | 0.8.3 | MIT OR Apache-2.0 | [Files](licenses/petgraph-0.8.3) |
| pharos | 0.5.3 | Unlicense | [Files](licenses/pharos-0.5.3) |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | [Files](licenses/pin-project-1.1.13) |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | [Files](licenses/pin-project-internal-1.1.13) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [Files](licenses/pin-project-lite-0.2.17) |
| piper | 0.2.5 | MIT OR Apache-2.0 | [Files](licenses/piper-0.2.5) |
| pkcs8 | 0.11.0 | Apache-2.0 OR MIT | [Files](licenses/pkcs8-0.11.0) |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | [Files](licenses/pkg-config-0.3.34) |
| plain | 0.2.3 | MIT/Apache-2.0 | [Files](licenses/plain-0.2.3) |
| plist | 1.10.1 | MIT | [Files](licenses/plist-1.10.1) |
| png | 0.18.1 | MIT OR Apache-2.0 | [Files](licenses/png-0.18.1) |
| polling | 3.11.0 | Apache-2.0 OR MIT | [Files](licenses/polling-3.11.0) |
| pollster | 0.4.0 | Apache-2.0/MIT | [Files](licenses/pollster-0.4.0) |
| polyval | 0.6.2 | Apache-2.0 OR MIT | [Files](licenses/polyval-0.6.2) |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | [Files](licenses/portable-atomic-1.15.0) |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | [Files](licenses/portable-atomic-util-0.2.8) |
| portmapper | 0.19.3 | MIT OR Apache-2.0 | [Files](licenses/portmapper-0.19.3) |
| postcard | 1.1.3 | MIT OR Apache-2.0 | [Files](licenses/postcard-1.1.3) |
| postcard-derive | 0.2.2 | MIT OR Apache-2.0 | [Files](licenses/postcard-derive-0.2.2) |
| potential_utf | 0.1.6 | Unicode-3.0 | [Files](licenses/potential_utf-0.1.6) |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | [Files](licenses/powerfmt-0.2.1) |
| pp-rs | 0.2.1 | BSD-3-Clause | [Files](licenses/pp-rs-0.2.1) |
| presser | 0.3.1 | MIT OR Apache-2.0 | [Files](licenses/presser-0.3.1) |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | [Files](licenses/proc-macro-crate-3.5.0) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [Files](licenses/proc-macro2-1.0.107) |
| profiling | 1.0.18 | MIT OR Apache-2.0 | [Files](licenses/profiling-1.0.18) |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | [Files](licenses/pxfm-0.1.30) |
| quick-xml | 0.41.0 | MIT | [Files](licenses/quick-xml-0.41.0) |
| quick-xml | 0.42.0 | MIT | [Files](licenses/quick-xml-0.42.0) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [Files](licenses/quote-1.0.47) |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [Files](licenses/r-efi-5.3.0) |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [Files](licenses/r-efi-6.0.0) |
| radsort | 0.1.1 | MIT OR Apache-2.0 | [Files](licenses/radsort-0.1.1) |
| rand | 0.10.3 | MIT OR Apache-2.0 | [Files](licenses/rand-0.10.3) |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | [Files](licenses/rand_core-0.10.1) |
| rand_distr | 0.6.0 | MIT OR Apache-2.0 | [Files](licenses/rand_distr-0.6.0) |
| rand_pcg | 0.10.2 | MIT OR Apache-2.0 | [Files](licenses/rand_pcg-0.10.2) |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | [Files](licenses/range-alloc-0.1.5) |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | [Files](licenses/raw-window-handle-0.6.2) |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/raw-window-metal-1.1.0) |
| read-fonts | 0.39.2 | MIT OR Apache-2.0 | [Files](licenses/read-fonts-0.39.2) |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | [Files](licenses/read-fonts-0.41.0) |
| rectangle-pack | 0.4.2 | MIT/Apache-2.0 | [Files](licenses/rectangle-pack-0.4.2) |
| redox_syscall | 0.4.1 | MIT | [Files](licenses/redox_syscall-0.4.1) |
| redox_syscall | 0.5.18 | MIT | [Files](licenses/redox_syscall-0.5.18) |
| redox_syscall | 0.9.4 | MIT | [Files](licenses/redox_syscall-0.9.4) |
| regex | 1.13.1 | MIT OR Apache-2.0 | [Files](licenses/regex-1.13.1) |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | [Files](licenses/regex-automata-0.4.18) |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | [Files](licenses/regex-syntax-0.8.11) |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/renderdoc-sys-1.1.0) |
| reqwest | 0.13.5 | MIT OR Apache-2.0 | [Files](licenses/reqwest-0.13.5) |
| rfd | 0.17.2 | MIT | [Files](licenses/rfd-0.17.2) |
| ring | 0.17.14 | Apache-2.0 AND ISC | [Files](licenses/ring-0.17.14) |
| rodio | 0.22.2 | MIT OR Apache-2.0 | [Files](licenses/rodio-0.22.2) |
| ron | 0.12.2 | MIT OR Apache-2.0 | [Files](licenses/ron-0.12.2) |
| rustc-hash | 1.1.0 | Apache-2.0/MIT | [Files](licenses/rustc-hash-1.1.0) |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | [Files](licenses/rustc-hash-2.1.3) |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | [Files](licenses/rustc_version-0.4.1) |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/rustix-0.38.44) |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/rustix-1.1.5) |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | [Files](licenses/rustls-0.23.46) |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | [Files](licenses/rustls-native-certs-0.8.5) |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | [Files](licenses/rustls-pki-types-1.15.1) |
| rustls-platform-verifier | 0.7.1 | MIT OR Apache-2.0 | [Files](licenses/rustls-platform-verifier-0.7.1) |
| rustls-platform-verifier-android | 0.2.0 | MIT OR Apache-2.0 | [Files](licenses/rustls-platform-verifier-android-0.2.0) |
| rustls-webpki | 0.103.15 | ISC | [Files](licenses/rustls-webpki-0.103.15) |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | [Files](licenses/rustversion-1.0.23) |
| ruzstd | 0.8.3 | MIT | [Files](licenses/ruzstd-0.8.3) |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | [Files](licenses/ryu-1.0.23) |
| same-file | 1.0.6 | Unlicense/MIT | [Files](licenses/same-file-1.0.6) |
| schannel | 0.1.29 | MIT | [Files](licenses/schannel-0.1.29) |
| scoped-tls | 1.0.1 | MIT/Apache-2.0 | [Files](licenses/scoped-tls-1.0.1) |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | [Files](licenses/scopeguard-1.2.0) |
| sctk-adwaita | 0.10.1 | MIT | [Files](licenses/sctk-adwaita-0.10.1) |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | [Files](licenses/security-framework-3.7.0) |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | [Files](licenses/security-framework-sys-2.17.0) |
| seize | 0.5.1 | MIT | [Files](licenses/seize-0.5.1) |
| semver | 1.0.28 | MIT OR Apache-2.0 | [Files](licenses/semver-1.0.28) |
| send_wrapper | 0.6.0 | MIT/Apache-2.0 | [Files](licenses/send_wrapper-0.6.0) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [Files](licenses/serde-1.0.229) |
| serde_bytes | 0.11.19 | MIT OR Apache-2.0 | [Files](licenses/serde_bytes-0.11.19) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [Files](licenses/serde_core-1.0.229) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [Files](licenses/serde_derive-1.0.229) |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | [Files](licenses/serde_json-1.0.151) |
| serde_spanned | 1.1.1 | MIT OR Apache-2.0 | [Files](licenses/serde_spanned-1.1.1) |
| serdect | 0.4.3 | Apache-2.0 OR MIT | [Files](licenses/serdect-0.4.3) |
| sha1_smol | 1.0.1 | BSD-3-Clause | [Files](licenses/sha1_smol-1.0.1) |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | [Files](licenses/sha2-0.11.1) |
| sharded-slab | 0.1.7 | MIT | [Files](licenses/sharded-slab-0.1.7) |
| shlex | 2.0.1 | MIT OR Apache-2.0 | [Files](licenses/shlex-2.0.1) |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | [Files](licenses/signal-hook-registry-1.4.8) |
| signature | 3.0.0 | Apache-2.0 OR MIT | [Files](licenses/signature-3.0.0) |
| simd-adler32 | 0.3.10 | MIT | [Files](licenses/simd-adler32-0.3.10) |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | [Files](licenses/simd_cesu8-1.2.0) |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | [Files](licenses/simdutf8-0.1.5) |
| simple-dns | 0.12.0 | MIT | [Files](licenses/simple-dns-0.12.0) |
| skrifa | 0.42.1 | MIT OR Apache-2.0 | [Files](licenses/skrifa-0.42.1) |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | [Files](licenses/skrifa-0.44.0) |
| slab | 0.4.12 | MIT | [Files](licenses/slab-0.4.12) |
| slotmap | 1.1.1 | Zlib | [Files](licenses/slotmap-1.1.1) |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | [Files](licenses/smallvec-1.16.2) |
| smithay-client-toolkit | 0.19.2 | MIT | [Files](licenses/smithay-client-toolkit-0.19.2) |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | [Files](licenses/smol_str-0.2.2) |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | [Files](licenses/socket2-0.6.5) |
| sorted-index-buffer | 0.2.1 | MIT OR Apache-2.0 | [Files](licenses/sorted-index-buffer-0.2.1) |
| spez | 0.1.2 | BSD-2-Clause | [Files](licenses/spez-0.1.2) |
| spin | 0.10.1 | MIT | [Files](licenses/spin-0.10.1) |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | [Files](licenses/spirv-0.4.0+sdk-1.4.341.0) |
| spki | 0.8.1 | Apache-2.0 OR MIT | [Files](licenses/spki-0.8.1) |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | [Files](licenses/stable_deref_trait-1.2.1) |
| stackfuture | 0.3.1 | MIT | [Files](licenses/stackfuture-0.3.1) |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/static_assertions-1.1.0) |
| strict-num | 0.1.1 | MIT | [Files](licenses/strict-num-0.1.1) |
| strum | 0.28.0 | MIT | [Files](licenses/strum-0.28.0) |
| strum_macros | 0.28.0 | MIT | [Files](licenses/strum_macros-0.28.0) |
| subtle | 2.6.1 | BSD-3-Clause | [Files](licenses/subtle-2.6.1) |
| svg_fmt | 0.4.5 | MIT/Apache-2.0 | [Files](licenses/svg_fmt-0.4.5) |
| swash | 0.2.10 | Apache-2.0 OR MIT | [Files](licenses/swash-0.2.10) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [Files](licenses/syn-2.0.119) |
| syn | 3.0.6 | MIT OR Apache-2.0 | [Files](licenses/syn-3.0.6) |
| sync_wrapper | 1.0.2 | Apache-2.0 | [Files](licenses/sync_wrapper-1.0.2) |
| synstructure | 0.14.0 | MIT | [Files](licenses/synstructure-0.14.0) |
| sys-locale | 0.3.2 | MIT OR Apache-2.0 | [Files](licenses/sys-locale-0.3.2) |
| sysinfo | 0.38.4 | MIT | [Files](licenses/sysinfo-0.38.4) |
| system-configuration | 0.8.0 | MIT OR Apache-2.0 | [Files](licenses/system-configuration-0.8.0) |
| system-configuration-sys | 0.6.0 | MIT OR Apache-2.0 | [Files](licenses/system-configuration-sys-0.6.0) |
| taffy | 0.10.1 | MIT | [Files](licenses/taffy-0.10.1) |
| termcolor | 1.4.1 | Unlicense OR MIT | [Files](licenses/termcolor-1.4.1) |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | [Files](licenses/thiserror-1.0.69) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [Files](licenses/thiserror-2.0.21) |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | [Files](licenses/thiserror-impl-1.0.69) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [Files](licenses/thiserror-impl-2.0.21) |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | [Files](licenses/thread_local-1.1.10) |
| time | 0.3.55 | MIT OR Apache-2.0 | [Files](licenses/time-0.3.55) |
| time-core | 0.1.9 | MIT OR Apache-2.0 | [Files](licenses/time-core-0.1.9) |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | [Files](licenses/time-macros-0.2.32) |
| tiny-skia | 0.11.4 | BSD-3-Clause | [Files](licenses/tiny-skia-0.11.4) |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | [Files](licenses/tiny-skia-path-0.11.4) |
| tinystr | 0.8.4 | Unicode-3.0 | [Files](licenses/tinystr-0.8.4) |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | [Files](licenses/tinyvec-1.13.3) |
| tokio | 1.53.2 | MIT | [Files](licenses/tokio-1.53.2) |
| tokio-macros | 2.7.2 | MIT | [Files](licenses/tokio-macros-2.7.2) |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | [Files](licenses/tokio-rustls-0.26.6) |
| tokio-stream | 0.1.19 | MIT | [Files](licenses/tokio-stream-0.1.19) |
| tokio-util | 0.7.20 | MIT | [Files](licenses/tokio-util-0.7.20) |
| tokio-websockets | 0.13.3 | MIT | [Files](licenses/tokio-websockets-0.13.3) |
| toml | 0.9.12+spec-1.1.0 | MIT OR Apache-2.0 | [Files](licenses/toml-0.9.12+spec-1.1.0) |
| toml_datetime | 0.7.5+spec-1.1.0 | MIT OR Apache-2.0 | [Files](licenses/toml_datetime-0.7.5+spec-1.1.0) |
| toml_datetime | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | [Files](licenses/toml_datetime-1.1.1+spec-1.1.0) |
| toml_edit | 0.25.15+spec-1.1.0 | MIT OR Apache-2.0 | [Files](licenses/toml_edit-0.25.15+spec-1.1.0) |
| toml_parser | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | [Files](licenses/toml_parser-1.1.3+spec-1.1.0) |
| toml_writer | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | [Files](licenses/toml_writer-1.1.2+spec-1.1.0) |
| tower | 0.5.3 | MIT | [Files](licenses/tower-0.5.3) |
| tower-http | 0.6.11 | MIT | [Files](licenses/tower-http-0.6.11) |
| tower-layer | 0.3.3 | MIT | [Files](licenses/tower-layer-0.3.3) |
| tower-service | 0.3.3 | MIT | [Files](licenses/tower-service-0.3.3) |
| tracing | 0.1.44 | MIT | [Files](licenses/tracing-0.1.44) |
| tracing-attributes | 0.1.31 | MIT | [Files](licenses/tracing-attributes-0.1.31) |
| tracing-core | 0.1.36 | MIT | [Files](licenses/tracing-core-0.1.36) |
| tracing-log | 0.2.0 | MIT | [Files](licenses/tracing-log-0.2.0) |
| tracing-oslog | 0.3.0 | Zlib | [Files](licenses/tracing-oslog-0.3.0) |
| tracing-subscriber | 0.3.23 | MIT | [Files](licenses/tracing-subscriber-0.3.23) |
| tracing-wasm | 0.2.1 | MIT OR Apache-2.0 | [Files](licenses/tracing-wasm-0.2.1) |
| tree_magic_mini | 3.2.2 | MIT | [Files](licenses/tree_magic_mini-3.2.2) |
| try-lock | 0.2.5 | MIT | [Files](licenses/try-lock-0.2.5) |
| ttf-parser | 0.25.1 | MIT OR Apache-2.0 | [Files](licenses/ttf-parser-0.25.1) |
| twox-hash | 2.1.5 | MIT | [Files](licenses/twox-hash-2.1.5) |
| typeid | 1.0.3 | MIT OR Apache-2.0 | [Files](licenses/typeid-1.0.3) |
| typenum | 1.20.1 | MIT OR Apache-2.0 | [Files](licenses/typenum-1.20.1) |
| typewit | 1.15.2 | Zlib | [Files](licenses/typewit-1.15.2) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [Files](licenses/unicode-ident-1.0.26) |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | [Files](licenses/unicode-segmentation-1.13.3) |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | [Files](licenses/unicode-width-0.2.2) |
| unicode-xid | 0.2.6 | MIT OR Apache-2.0 | [Files](licenses/unicode-xid-0.2.6) |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | [Files](licenses/universal-hash-0.5.1) |
| untrusted | 0.9.0 | ISC | [Files](licenses/untrusted-0.9.0) |
| url | 2.5.8 | MIT OR Apache-2.0 | [Files](licenses/url-2.5.8) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | [Files](licenses/utf8_iter-1.0.4) |
| uuid | 1.27.0 | Apache-2.0 OR MIT | [Files](licenses/uuid-1.27.0) |
| valuable | 0.1.1 | MIT | [Files](licenses/valuable-0.1.1) |
| variadics_please | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/variadics_please-1.1.0) |
| vec_map | 0.8.2 | MIT/Apache-2.0 | [Files](licenses/vec_map-0.8.2) |
| version_check | 0.9.5 | MIT/Apache-2.0 | [Files](licenses/version_check-0.9.5) |
| walkdir | 2.5.0 | Unlicense/MIT | [Files](licenses/walkdir-2.5.0) |
| want | 0.3.2 | MIT | [Files](licenses/want-0.3.2) |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/wasi-0.11.1+wasi-snapshot-preview1) |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/wasip2-1.0.4+wasi-0.2.12) |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | [Files](licenses/wasm-bindgen-0.2.129) |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | [Files](licenses/wasm-bindgen-futures-0.4.79) |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | [Files](licenses/wasm-bindgen-macro-0.2.129) |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | [Files](licenses/wasm-bindgen-macro-support-0.2.129) |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | [Files](licenses/wasm-bindgen-shared-0.2.129) |
| wasm-streams | 0.5.0 | MIT OR Apache-2.0 | [Files](licenses/wasm-streams-0.5.0) |
| wayland-backend | 0.3.17 | MIT | [Files](licenses/wayland-backend-0.3.17) |
| wayland-client | 0.31.15 | MIT | [Files](licenses/wayland-client-0.31.15) |
| wayland-csd-frame | 0.3.0 | MIT | [Files](licenses/wayland-csd-frame-0.3.0) |
| wayland-cursor | 0.31.14 | MIT | [Files](licenses/wayland-cursor-0.31.14) |
| wayland-protocols | 0.32.13 | MIT | [Files](licenses/wayland-protocols-0.32.13) |
| wayland-protocols-plasma | 0.3.12 | MIT | [Files](licenses/wayland-protocols-plasma-0.3.12) |
| wayland-protocols-wlr | 0.3.12 | MIT | [Files](licenses/wayland-protocols-wlr-0.3.12) |
| wayland-scanner | 0.31.11 | MIT | [Files](licenses/wayland-scanner-0.31.11) |
| wayland-sys | 0.31.11 | MIT | [Files](licenses/wayland-sys-0.31.11) |
| weak-table | 0.3.2 | MIT | [Files](licenses/weak-table-0.3.2) |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | [Files](licenses/web-sys-0.3.106) |
| web-task | 1.1.3 | MIT OR Apache-2.0 | [Files](licenses/web-task-1.1.3) |
| web-time | 1.1.0 | MIT OR Apache-2.0 | [Files](licenses/web-time-1.1.0) |
| webpki-root-certs | 1.0.9 | CDLA-Permissive-2.0 | [Files](licenses/webpki-root-certs-1.0.9) |
| webpki-roots | 1.0.9 | CDLA-Permissive-2.0 | [Files](licenses/webpki-roots-1.0.9) |
| wgpu | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-29.0.4) |
| wgpu-core | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-core-29.0.4) |
| wgpu-core-deps-apple | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-core-deps-apple-29.0.4) |
| wgpu-core-deps-wasm | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-core-deps-wasm-29.0.4) |
| wgpu-core-deps-windows-linux-android | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-core-deps-windows-linux-android-29.0.4) |
| wgpu-hal | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-hal-29.0.4) |
| wgpu-naga-bridge | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-naga-bridge-29.0.4) |
| wgpu-types | 29.0.4 | MIT OR Apache-2.0 | [Files](licenses/wgpu-types-29.0.4) |
| widestring | 1.2.1 | MIT OR Apache-2.0 | [Files](licenses/widestring-1.2.1) |
| winapi | 0.3.9 | MIT/Apache-2.0 | [Files](licenses/winapi-0.3.9) |
| winapi-i686-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | [Files](licenses/winapi-i686-pc-windows-gnu-0.4.0) |
| winapi-util | 0.1.11 | Unlicense OR MIT | [Files](licenses/winapi-util-0.1.11) |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 | [Files](licenses/winapi-x86_64-pc-windows-gnu-0.4.0) |
| windows | 0.62.2 | MIT OR Apache-2.0 | [Files](licenses/windows-0.62.2) |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | [Files](licenses/windows-collections-0.3.2) |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | [Files](licenses/windows-core-0.62.2) |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | [Files](licenses/windows-future-0.3.2) |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | [Files](licenses/windows-implement-0.60.2) |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | [Files](licenses/windows-interface-0.59.3) |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | [Files](licenses/windows-link-0.2.1) |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | [Files](licenses/windows-numerics-0.3.1) |
| windows-registry | 0.6.1 | MIT OR Apache-2.0 | [Files](licenses/windows-registry-0.6.1) |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | [Files](licenses/windows-result-0.4.1) |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | [Files](licenses/windows-strings-0.5.1) |
| windows-sys | 0.45.0 | MIT OR Apache-2.0 | [Files](licenses/windows-sys-0.45.0) |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | [Files](licenses/windows-sys-0.52.0) |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | [Files](licenses/windows-sys-0.59.0) |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 | [Files](licenses/windows-sys-0.60.2) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [Files](licenses/windows-sys-0.61.2) |
| windows-targets | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows-targets-0.42.2) |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows-targets-0.52.6) |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 | [Files](licenses/windows-targets-0.53.5) |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | [Files](licenses/windows-threading-0.2.1) |
| windows_aarch64_gnullvm | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_aarch64_gnullvm-0.42.2) |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_aarch64_gnullvm-0.52.6) |
| windows_aarch64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_aarch64_gnullvm-0.53.1) |
| windows_aarch64_msvc | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_aarch64_msvc-0.42.2) |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_aarch64_msvc-0.52.6) |
| windows_aarch64_msvc | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_aarch64_msvc-0.53.1) |
| windows_i686_gnu | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_gnu-0.42.2) |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_gnu-0.52.6) |
| windows_i686_gnu | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_gnu-0.53.1) |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_gnullvm-0.52.6) |
| windows_i686_gnullvm | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_gnullvm-0.53.1) |
| windows_i686_msvc | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_msvc-0.42.2) |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_msvc-0.52.6) |
| windows_i686_msvc | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_i686_msvc-0.53.1) |
| windows_x86_64_gnu | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_gnu-0.42.2) |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_gnu-0.52.6) |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_gnu-0.53.1) |
| windows_x86_64_gnullvm | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_gnullvm-0.42.2) |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_gnullvm-0.52.6) |
| windows_x86_64_gnullvm | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_gnullvm-0.53.1) |
| windows_x86_64_msvc | 0.42.2 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_msvc-0.42.2) |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_msvc-0.52.6) |
| windows_x86_64_msvc | 0.53.1 | MIT OR Apache-2.0 | [Files](licenses/windows_x86_64_msvc-0.53.1) |
| winit | 0.30.13 | Apache-2.0 | [Files](licenses/winit-0.30.13) |
| winnow | 0.7.15 | MIT | [Files](licenses/winnow-0.7.15) |
| winnow | 1.0.4 | MIT | [Files](licenses/winnow-1.0.4) |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [Files](licenses/wit-bindgen-0.57.1) |
| wl-clipboard-rs | 0.9.4 | MIT OR Apache-2.0 | [Files](licenses/wl-clipboard-rs-0.9.4) |
| wmi | 0.18.4 | MIT OR Apache-2.0 | [Files](licenses/wmi-0.18.4) |
| writeable | 0.6.4 | Unicode-3.0 | [Files](licenses/writeable-0.6.4) |
| ws_stream_wasm | 0.7.5 | Unlicense | [Files](licenses/ws_stream_wasm-0.7.5) |
| x11-dl | 2.21.0 | MIT | [Files](licenses/x11-dl-2.21.0) |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | [Files](licenses/x11rb-0.13.2) |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | [Files](licenses/x11rb-protocol-0.13.2) |
| xcursor | 0.3.11 | MIT | [Files](licenses/xcursor-0.3.11) |
| xkbcommon-dl | 0.4.2 | MIT | [Files](licenses/xkbcommon-dl-0.4.2) |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | [Files](licenses/xkeysym-0.2.1) |
| xml-rs | 0.8.29 | MIT | [Files](licenses/xml-rs-0.8.29) |
| xmltree | 0.10.3 | MIT | [Files](licenses/xmltree-0.10.3) |
| yazi | 0.2.1 | Apache-2.0 OR MIT | [Files](licenses/yazi-0.2.1) |
| yoke | 0.8.3 | Unicode-3.0 | [Files](licenses/yoke-0.8.3) |
| yoke-derive | 0.8.4 | Unicode-3.0 | [Files](licenses/yoke-derive-0.8.4) |
| zeno | 0.3.3 | Apache-2.0 OR MIT | [Files](licenses/zeno-0.3.3) |
| zerocopy | 0.8.61 | BSD-2-Clause OR Apache-2.0 OR MIT | [Files](licenses/zerocopy-0.8.61) |
| zerocopy-derive | 0.8.61 | BSD-2-Clause OR Apache-2.0 OR MIT | [Files](licenses/zerocopy-derive-0.8.61) |
| zerofrom | 0.1.8 | Unicode-3.0 | [Files](licenses/zerofrom-0.1.8) |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | [Files](licenses/zerofrom-derive-0.1.8) |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | [Files](licenses/zeroize-1.9.1) |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | [Files](licenses/zeroize_derive-1.5.0) |
| zerotrie | 0.2.5 | Unicode-3.0 | [Files](licenses/zerotrie-0.2.5) |
| zerovec | 0.11.8 | Unicode-3.0 | [Files](licenses/zerovec-0.11.8) |
| zerovec-derive | 0.11.6 | Unicode-3.0 | [Files](licenses/zerovec-derive-0.11.6) |
| zlib-rs | 0.6.8 | Zlib | [Files](licenses/zlib-rs-0.6.8) |
| zmij | 1.0.23 | MIT | [Files](licenses/zmij-1.0.23) |
