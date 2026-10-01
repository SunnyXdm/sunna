package dev.sunna.app

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.SecureRandom
import kotlin.math.roundToInt

/** A computer you've added: where it is, its key, and what it last told us
 *  about itself. Stored as the desktop app stores them (machines.json). */
data class Machine(
    val id: String,
    val name: String,
    /** As entered: "100.124.64.79", "archlinux:48800", "[fd7a::1]". */
    val address: String,
    val key: String,
    val os: String = "",
    val device: String = "",
    val model: String = "",
    val width: Int = 0,
    val height: Int = 0,
    val added: Long = 0,
    val lastSeen: Long = 0,
    val lastConnected: Long = 0,
) {
    fun toJson(): JSONObject = JSONObject()
        .put("id", id).put("name", name).put("address", address).put("key", key)
        .put("os", os).put("device", device).put("model", model)
        .put("width", width).put("height", height)
        .put("added", added).put("last_seen", lastSeen).put("last_connected", lastConnected)

    companion object {
        fun from(json: JSONObject) = Machine(
            id = json.optString("id"),
            name = json.optString("name"),
            address = json.optString("address"),
            key = json.optString("key"),
            os = json.optString("os"),
            device = json.optString("device"),
            model = json.optString("model"),
            width = json.optInt("width"),
            height = json.optInt("height"),
            added = json.optLong("added"),
            lastSeen = json.optLong("last_seen"),
            lastConnected = json.optLong("last_connected"),
        )
    }
}

/** One look at a computer (the Rust side's `reach::Check`). */
data class Check(
    /** checking | ready | busy | wrong-key | update-needed | unreachable | not-found | invalid */
    val state: String,
    val name: String = "",
    val os: String = "",
    val device: String = "",
    val model: String = "",
    val width: Int = 0,
    val height: Int = 0,
    val rttMs: Double? = null,
    val detail: String = "",
    /** When busy: who's connected, if the host says. */
    val viewer: String = "",
) {
    val lit: Boolean get() = state in LIT

    companion object {
        /** States in which a computer's screen is lit: something answered. */
        val LIT = setOf("ready", "busy", "wrong-key", "update-needed")
        val CHECKING = Check("checking")

        fun from(json: JSONObject) = Check(
            state = json.optString("state", "unreachable"),
            name = json.optString("name"),
            os = json.optString("os"),
            device = json.optString("device"),
            model = json.optString("model"),
            width = json.optInt("width"),
            height = json.optInt("height"),
            rttMs = if (json.isNull("rtt_ms")) null else json.optDouble("rtt_ms"),
            detail = json.optString("detail"),
            viewer = json.optString("viewer"),
        )

        fun parse(text: String): Check = runCatching { from(JSONObject(text)) }.getOrDefault(Check("unreachable"))
    }
}

/** What the add and edit sheets save. */
data class Draft(val name: String, val address: String, val key: String, val about: Check?)

/** The host part of an address as typed: the name a computer gets when the
 *  host hasn't told us its own. */
fun hostOf(address: String): String {
    val text = address.trim().removePrefix("sunna://").split('?', '/', '#').first()
    if (text.startsWith("[")) return text.substring(1, text.indexOf(']').takeIf { it > 0 } ?: text.length)
    return if (text.count { it == ':' } == 1) text.substringBefore(':') else text
}

/** A `sunna://host?key=...` link, split into its address and key. */
fun splitLink(text: String): Pair<String, String>? {
    val trimmed = text.trim()
    if (!trimmed.startsWith("sunna://")) return null
    val key = Regex("""[?&]key=([^&#\s]*)""").find(trimmed)?.groupValues?.get(1) ?: ""
    val address = trimmed.removePrefix("sunna://").split('?', '#').first().trimEnd('/')
    return address to android.net.Uri.decode(key)
}

private val DEVICES = mapOf("laptop" to "Laptop", "desktop" to "Desktop", "vm" to "VM", "server" to "Server")

/** "Arch Linux · Desktop", "macOS 26.0 · MacBook Air"; the address when we
 *  don't know what it is yet. */
fun systemText(machine: Machine, check: Check?): String {
    val os = check?.os?.ifEmpty { null } ?: machine.os
    if (os.isEmpty()) return machine.address
    val model = check?.model?.ifEmpty { null } ?: machine.model
    val device = DEVICES[check?.device?.ifEmpty { null } ?: machine.device]
    return listOfNotNull(os, model.ifEmpty { device }).joinToString(" · ")
}

fun ms(value: Double): String = if (value < 1) "<1 ms" else "${value.roundToInt()} ms"

private fun ago(seconds: Long): String {
    val diff = System.currentTimeMillis() / 1000 - seconds
    return when {
        diff < 90 -> "just now"
        diff < 3600 -> "${(diff / 60.0).roundToInt()} min ago"
        diff < 86400 -> "${(diff / 3600.0).roundToInt()} h ago"
        else -> ((diff / 86400.0).roundToInt()).let { if (it == 1) "yesterday" else "$it days ago" }
    }
}

/** A computer's status line, in the desktop app's words. */
fun describe(machine: Machine?, check: Check?): String = when (check?.state ?: "checking") {
    "idle" -> "Enter its address"
    "checking" -> "Looking…"
    "ready" -> check?.rttMs?.let { "Ready · ${ms(it)}" } ?: "Ready"
    "busy" -> check?.viewer?.ifEmpty { null }?.let { "In use · $it" } ?: "In use"
    "wrong-key" -> "Key doesn't match"
    "update-needed" -> "Needs an update"
    "not-found" -> "Address not found"
    "invalid" -> "Address isn't valid"
    else -> machine?.lastSeen?.takeIf { it > 0 }?.let { "Offline · seen ${ago(it)}" } ?: "Not responding"
}

fun statusColor(state: String): Int = when (state) {
    "ready" -> Palette.READY
    "busy", "update-needed" -> Palette.BUSY
    "wrong-key" -> Palette.DANGER
    else -> Palette.TEXT_3
}

/** The computers you've added, in this app's private files (the keys are
 *  secrets; backups leave them out). */
class MachineStore(context: Context) {
    private val file = File(context.filesDir, "machines.json")
    private var cached: List<Machine>? = null

    @Synchronized
    fun list(): List<Machine> {
        cached?.let { return it }
        val machines = runCatching {
            val array = JSONObject(file.readText()).optJSONArray("machines") ?: JSONArray()
            (0 until array.length()).map { Machine.from(array.getJSONObject(it)) }
        }.getOrDefault(emptyList())
        cached = machines
        return machines
    }

    @Synchronized
    private fun save(machines: List<Machine>) {
        val array = JSONArray()
        machines.forEach { array.put(it.toJson()) }
        val temporary = File(file.parentFile, "machines.json.tmp")
        temporary.writeText(JSONObject().put("machines", array).toString(2))
        temporary.renameTo(file)
        cached = machines
    }

    private fun now() = System.currentTimeMillis() / 1000

    private fun newId(): String {
        val bytes = ByteArray(8)
        SecureRandom().nextBytes(bytes)
        return bytes.joinToString("") { "%02x".format(it) }
    }

    private fun nameFor(draft: Draft): String =
        draft.name.trim().ifEmpty { draft.about?.name?.ifEmpty { null } ?: hostOf(draft.address) }

    private fun learn(machine: Machine, check: Check?): Machine {
        if (check == null) return machine
        return machine.copy(
            os = check.os.ifEmpty { machine.os },
            device = check.device.ifEmpty { machine.device },
            model = check.model.ifEmpty { machine.model },
            width = if (check.width > 0 && check.height > 0) check.width else machine.width,
            height = if (check.width > 0 && check.height > 0) check.height else machine.height,
            lastSeen = if (check.state == "ready" || check.state == "busy") now() else machine.lastSeen,
        )
    }

    @Synchronized
    fun add(draft: Draft): Machine {
        val machine = learn(Machine(newId(), nameFor(draft), draft.address.trim(), draft.key.trim(), added = now()), draft.about)
        save(list() + machine)
        return machine
    }

    @Synchronized
    fun update(id: String, draft: Draft): Machine? {
        var updated: Machine? = null
        save(list().map { machine ->
            if (machine.id != id) return@map machine
            val moved = machine.address != draft.address.trim()
            // A different address may be a different computer.
            val base = if (moved) machine.copy(os = "", device = "", model = "", width = 0, height = 0, lastSeen = 0) else machine
            learn(base.copy(name = nameFor(draft), address = draft.address.trim(), key = draft.key.trim()), draft.about).also { updated = it }
        })
        return updated
    }

    @Synchronized
    fun remove(id: String) = save(list().filter { it.id != id })

    @Synchronized
    fun put(index: Int, machine: Machine) {
        val machines = list().filter { it.id != machine.id }.toMutableList()
        machines.add(index.coerceIn(0, machines.size), machine)
        save(machines)
    }

    /** Keep what the computers said about themselves. */
    @Synchronized
    fun learn(checks: Map<String, Check>) {
        val machines = list()
        val learned = machines.map { learn(it, checks[it.id]) }
        // Only when something a tile shows changed, not every round.
        if (learned.zip(machines).any { (a, b) -> a.copy(lastSeen = 0) != b.copy(lastSeen = 0) || (a.lastSeen - b.lastSeen) > 600 }) save(learned)
    }

    @Synchronized
    fun markConnected(id: String) = save(list().map { if (it.id == id) it.copy(lastConnected = now(), lastSeen = now()) else it })
}

/** Small preferences: how touch works, what the session shows. */
class Prefs(context: Context) {
    private val prefs = context.getSharedPreferences("sunna", Context.MODE_PRIVATE)
    private val tablet = context.resources.configuration.smallestScreenWidthDp >= 600

    /** "trackpad" (the screen is a trackpad, the pointer moves) or "touch"
     *  (tap where you want to click). */
    var touchMode: String
        get() = prefs.getString("touch_mode", null) ?: if (tablet) "touch" else "trackpad"
        set(value) = prefs.edit().putString("touch_mode", value).apply()

    var stats: Boolean
        get() = prefs.getBoolean("stats", false)
        set(value) = prefs.edit().putBoolean("stats", value).apply()

    var sound: Boolean
        get() = prefs.getBoolean("sound", true)
        set(value) = prefs.edit().putBoolean("sound", value).apply()

    /** Stream size as a percentage of the host's display (100 = sharp). */
    var scale: Int
        get() = prefs.getInt("scale", 100)
        set(value) = prefs.edit().putInt("scale", value).apply()

    /** 0 for the host's own choice. */
    var bitrateMbps: Int
        get() = prefs.getInt("bitrate", 0)
        set(value) = prefs.edit().putInt("bitrate", value).apply()

    var fps: Int
        get() = prefs.getInt("fps", 60)
        set(value) = prefs.edit().putInt("fps", value).apply()

    /** "" for this phone's best (HEVC when it decodes it in hardware). */
    var codec: String
        get() = prefs.getString("codec", "") ?: ""
        set(value) = prefs.edit().putString("codec", value).apply()
}
