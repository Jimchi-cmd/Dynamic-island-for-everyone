package islet.island

import java.net.URLEncoder
import java.util.Locale

/** The sky, as the island draws it. */
enum class Sky { Clear, PartlyCloudy, Cloudy, Fog, Drizzle, Rain, Snow, Storm }

data class Weather(
    val celsius: Double,
    val code: Int,
    val day: Boolean,
    val highC: Double?,
    val lowC: Double?,
) {
    val sky get() = Forecast.sky(code)
    val words get() = Forecast.words(code)
}

data class Place(val name: String, val region: String, val country: String, val lat: Double, val lon: Double) {
    /** "Kampala, Uganda". */
    val label get() = listOf(name, region.takeIf { it.isNotEmpty() && it != name }, country)
        .filterNotNull().filter { it.isNotEmpty() }.distinct().joinToString(", ")
}

/** Open-Meteo: free weather with no key and no account. */
object Forecast {
    fun url(lat: Double, lon: Double): String =
        String.format(
            Locale.ROOT,
            "https://api.open-meteo.com/v1/forecast?latitude=%.4f&longitude=%.4f" +
                "&current=temperature_2m,weather_code,is_day&daily=temperature_2m_max,temperature_2m_min" +
                "&timezone=auto&forecast_days=1",
            lat,
            lon,
        )

    fun searchUrl(name: String): String =
        "https://geocoding-api.open-meteo.com/v1/search?name=${URLEncoder.encode(name.trim(), "UTF-8")}&count=6&language=en&format=json"

    fun parse(json: String): Weather? {
        val v = Json.parse(json) ?: return null
        val t = v.at("current", "temperature_2m") as? Double ?: return null
        val code = (v.at("current", "weather_code") as? Double)?.toInt() ?: 0
        val day = (v.at("current", "is_day") as? Double)?.let { it > 0 } ?: true
        return Weather(t, code, day, v.at("daily", "temperature_2m_max", 0) as? Double, v.at("daily", "temperature_2m_min", 0) as? Double)
    }

    fun places(json: String): List<Place> {
        val results = Json.parse(json).at("results") as? List<*> ?: return emptyList()
        return results.mapNotNull { r ->
            val name = r.at("name") as? String ?: return@mapNotNull null
            val lat = r.at("latitude") as? Double ?: return@mapNotNull null
            val lon = r.at("longitude") as? Double ?: return@mapNotNull null
            Place(name, r.at("admin1") as? String ?: "", r.at("country") as? String ?: "", lat, lon)
        }
    }

    /** WMO weather codes, as Open-Meteo uses them. */
    fun sky(code: Int): Sky = when (code) {
        0, 1 -> Sky.Clear
        2 -> Sky.PartlyCloudy
        3 -> Sky.Cloudy
        45, 48 -> Sky.Fog
        51, 53, 55, 56, 57 -> Sky.Drizzle
        61, 63, 65, 66, 67, 80, 81, 82 -> Sky.Rain
        71, 73, 75, 77, 85, 86 -> Sky.Snow
        95, 96, 99 -> Sky.Storm
        else -> Sky.Cloudy
    }

    fun words(code: Int): String = when (code) {
        0 -> "Clear"
        1 -> "Mostly clear"
        2 -> "Partly cloudy"
        3 -> "Cloudy"
        45, 48 -> "Fog"
        51, 53, 55 -> "Drizzle"
        56, 57 -> "Freezing drizzle"
        61, 80 -> "Light rain"
        63, 81 -> "Rain"
        65, 82 -> "Heavy rain"
        66, 67 -> "Freezing rain"
        71, 85 -> "Light snow"
        73 -> "Snow"
        75, 86 -> "Heavy snow"
        77 -> "Snow grains"
        95 -> "Thunderstorm"
        96, 99 -> "Thunderstorm with hail"
        else -> "Cloudy"
    }
}
