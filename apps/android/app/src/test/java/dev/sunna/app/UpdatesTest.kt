package dev.sunna.app
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Test
class UpdatesTest {
    @Test fun sessionReceivesExactlyTheApprovedBytes() {
        val approved = "abc".toByteArray()
        val output = java.io.ByteArrayOutputStream()
        Updates.copyApprovedApk(approved.inputStream(), output, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        org.junit.Assert.assertArrayEquals(approved, output.toByteArray())
    }
    @Test(expected = IllegalArgumentException::class)
    fun changedApkFailsBeforeSessionCommit() {
        Updates.copyApprovedApk("changed".byteInputStream(), java.io.ByteArrayOutputStream(), "0".repeat(64))
    }
    @Test fun versionOrderingIsNumeric() {
        assertTrue(Updates.compare("0.10.0", "0.9.99") > 0)
        assertTrue(Updates.compare("1.0.0", "0.999.999") > 0)
        assertEquals(0, Updates.compare("1.2.3", "1.2.3"))
        assertTrue(Updates.compare("1.2.2", "1.2.3") < 0)
    }
}
