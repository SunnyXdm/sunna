package dev.sunna.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class IdentityTest {
    @Test fun linkCarriesIdentityAndLegacyLinksHaveNone() {
        val pin = "A".repeat(43)
        val link = splitLink("sunna://[::1]:48800?key=fixture%20value&id=$pin")!!
        assertEquals("[::1]:48800", link.first)
        assertEquals("fixture value", link.second)
        assertEquals(pin, link.third)
        assertNull(splitLink("sunna://host?key=fixture")!!.third)
    }

    @Test fun onlyNeverVerifiedRecordsOfferTheException() {
        val record = Machine("a", "Office", "localhost", "fixture")
        assertEquals(true, record.canAllowOlder())
        assertEquals(false, record.copy(upgraded = true).canAllowOlder())
        assertEquals(false, record.copy(identity = "A".repeat(43)).canAllowOlder())
        assertEquals(false, record.canAllowOlder("A".repeat(43)))
    }

    @Test fun editingEitherCredentialInvalidatesEvenBeforeTheNextCheckRuns() {
        val probes = DraftChecks()
        val ready = Check("ready", identity = "A".repeat(43))
        for ((address, key) in listOf("b" to "key", "a" to "changed")) {
            val first = probes.schedule("a", "key")
            assertEquals(true, probes.accept(first, ready))
            val second = probes.schedule(address, key)
            assertNull(probes.result)
            assertEquals(false, probes.accept(first, ready))
            assertNull(probes.result)
            probes.cancel()
            assertEquals(false, probes.accept(second, ready))
        }
    }

    @Test fun hidingStatsKeepsTheLegacySessionLabel() {
        assertEquals("Not fully verified", sessionLabel(false, "60 fps", true))
        assertNull(sessionLabel(false, "60 fps", false))
        assertEquals("60 fps", sessionLabel(true, "60 fps", false))
    }

    @Test fun verificationStatesUseTheStatusLine() {
        assertEquals(describe(null, Check("wrong-key")), describe(null, Check("wrong-key", unverified = true)))
        assertEquals("Identity changed", describe(null, Check("identity-changed")))
        assertEquals("Needs an update", describe(null, Check("update-needed")))
        assertEquals("Not fully verified", describe(null, Check("ready", unverified = true)))
    }
}
