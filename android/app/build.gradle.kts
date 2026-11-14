plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "io.github.jimchi_cmd.islet"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.jimchi_cmd.islet"
        minSdk = 26
        targetSdk = 35
        versionCode = 2
        versionName = "0.3.1"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    // Releases are signed when the workflow hands over a keystore (see
    // docs/phone.md). The keystore itself is never in the repository.
    val keystorePath = System.getenv("ISLET_ANDROID_KEYSTORE")
    if (keystorePath != null) {
        signingConfigs {
            create("release") {
                storeFile = file(keystorePath)
                storePassword = System.getenv("ISLET_ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ISLET_ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ISLET_ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (keystorePath != null) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
    }

    sourceSets["main"].java.srcDirs("src/main/kotlin")
}

dependencies {
    implementation(project(":protocol"))
    implementation("androidx.appcompat:appcompat:1.7.0")
    // QR scanning (Apache-2.0).
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
}
