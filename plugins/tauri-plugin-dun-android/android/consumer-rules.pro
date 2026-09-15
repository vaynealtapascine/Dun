# Merged into the app's R8 configuration.

# Receivers, the plugin and the JNI holder are referenced from the manifest,
# from Tauri by reflection, and from native code by name.
-keep class app.dun.android.** { *; }

# Rust exports are looked up as Java_<package>_<class>_<method>; renaming a
# native method or its class breaks the link at runtime.
-keepclasseswithmembernames class * {
    native <methods>;
}
