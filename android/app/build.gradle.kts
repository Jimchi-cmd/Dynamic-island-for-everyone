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
        versionCode = 1
        versionName = "0.3.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
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
