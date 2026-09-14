plugins {
    id("com.android.application")
}

android {
    namespace = "com.syngui.calculator"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.syngui.calculator"
        minSdk = 30
        targetSdk = 35
        versionCode = 1
        versionName = "1.0.0"
    }

    signingConfigs {
        create("release") {
            storeFile = file(System.getProperty("user.home") + "/.android/debug.keystore")
            storePassword = "android"
            keyAlias = "androiddebugkey"
            keyPassword = "android"
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("release")
        }
    }
}

dependencies {
    // Версия обязана совпадать с GameActivity, вшитой в android-activity (0.6.1 → 4.4.0),
    // иначе RegisterNatives падает на старте: NoSuchMethodError onTouchEventNative.
    implementation("androidx.games:games-activity:4.4.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation(platform("org.jetbrains.kotlin:kotlin-bom:2.1.10"))
}
