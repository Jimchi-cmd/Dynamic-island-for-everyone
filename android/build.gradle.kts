// The Islet companion for Android. Two modules:
//  - protocol: the encrypted link to the computer, in plain Kotlin (tested
//    on any JVM against the same vectors as the Rust side);
//  - app: the Android app around it.
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
    id("org.jetbrains.kotlin.jvm") version "2.0.21" apply false
}
