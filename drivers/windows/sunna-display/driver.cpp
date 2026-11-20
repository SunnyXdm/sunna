#define NOMINMAX
#include <windows.h>
#include <wudfwdm.h>
#include <wdf.h>
#include <iddcx.h>
#include <dxgi1_4.h>
#include <d3d11.h>
#include <avrt.h>
#include <wrl/client.h>
#include <new>
#include <stdio.h>
#include "modes.h"

using Microsoft::WRL::ComPtr;
using sunna::Mode;
using sunna::Modes;

static_assert(sizeof(SUNNA_DISPLAY_INFO) == 8);
static_assert(sizeof(SUNNA_DISPLAY_ADD) == 16);
static_assert(sizeof(SUNNA_DISPLAY_ID) == 4);
static const GUID InterfaceId = SUNNA_DISPLAY_INTERFACE_GUID_INIT;

static void Log(const wchar_t* message, ULONG code = 0) noexcept {
    wchar_t text[256];
    swprintf_s(text, L"SunnaDisplay: %s (0x%08lX)\n", message, code);
    OutputDebugStringW(text);
}
class Lock final {
    SRWLOCK* lock_;
public:
    explicit Lock(SRWLOCK& lock) noexcept : lock_(&lock) { AcquireSRWLockExclusive(lock_); }
    ~Lock() { ReleaseSRWLockExclusive(lock_); }
    Lock(const Lock&) = delete;
    Lock& operator=(const Lock&) = delete;
};
class Handle final {
    HANDLE value_ = nullptr;
public:
    Handle() = default;
    ~Handle() { Reset(); }
    Handle(const Handle&) = delete;
    Handle& operator=(const Handle&) = delete;
    HANDLE Get() const noexcept { return value_; }
    void Reset(HANDLE value = nullptr) noexcept {
        if (value_) CloseHandle(value_);
        value_ = value;
    }
};

struct MonitorState;
struct DriverState {
    SRWLOCK registryLock;
    MonitorState* monitors;
    UINT nextId;
};
struct DeviceState {
    SRWLOCK operationLock;
    IDDCX_ADAPTER adapter;
    IDDCX_MONITOR monitors[SUNNA_DISPLAY_MAX_MONITORS];
    bool powered;
};
struct AdapterState { volatile LONG ready; };
class FramePump;
struct MonitorState {
    SRWLOCK pumpLock;
    FramePump* pump;
    IDDCX_MONITOR object;
    UINT id;
    Modes modes;
    BYTE edid[128];
    MonitorState* next;
    bool registered;
    bool closing;
    // Keep the event alive for the monitor, including gaps between swapchains.
    HANDLE cursorEvent;
};
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DriverState, GetDriverState);
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(DeviceState, GetDeviceState);
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(AdapterState, GetAdapterState);
WDF_DECLARE_CONTEXT_TYPE_WITH_NAME(MonitorState, GetMonitorState);

extern "C" DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_DEVICE_ADD DeviceAdd;
EVT_WDF_DEVICE_D0_ENTRY EnterD0;
EVT_WDF_DEVICE_D0_EXIT LeaveD0;
EVT_WDF_DEVICE_RELEASE_HARDWARE ReleaseHardware;
EVT_WDF_OBJECT_CONTEXT_CLEANUP DeviceCleanup;
EVT_WDF_OBJECT_CONTEXT_CLEANUP MonitorCleanup;
EVT_WDF_OBJECT_CONTEXT_DESTROY MonitorDestroy;
EVT_IDD_CX_DEVICE_IO_CONTROL DeviceControl;
EVT_IDD_CX_ADAPTER_INIT_FINISHED AdapterReady;
EVT_IDD_CX_ADAPTER_COMMIT_MODES CommitModes;
EVT_IDD_CX_PARSE_MONITOR_DESCRIPTION ParseDescription;
EVT_IDD_CX_MONITOR_GET_DEFAULT_DESCRIPTION_MODES DefaultModes;
EVT_IDD_CX_MONITOR_QUERY_TARGET_MODES TargetModes;
EVT_IDD_CX_MONITOR_ASSIGN_SWAPCHAIN AssignSwapChain;
EVT_IDD_CX_MONITOR_UNASSIGN_SWAPCHAIN UnassignSwapChain;

static DriverState* Registry() noexcept { return GetDriverState(WdfGetDriver()); }

class FramePump final {
    IDDCX_MONITOR monitor_;
    IDDCX_SWAPCHAIN chain_;
    LUID luid_;
    HANDLE frameEvent_;
    HANDLE cursorEvent_;
    Handle stop_;
    Handle thread_;
    DWORD lastShape_ = 0;
    BYTE cursorBytes_[256 * 256 * 4]{};

    bool ReadCursor() noexcept {
        IDARG_IN_QUERY_HWCURSOR input{};
        input.LastShapeId = lastShape_;
        input.ShapeBufferSizeInBytes = sizeof(cursorBytes_);
        input.pShapeBuffer = cursorBytes_;
        IDARG_OUT_QUERY_HWCURSOR output{};
        const NTSTATUS status = IddCxMonitorQueryHardwareCursor(monitor_, &input, &output);
        if (!NT_SUCCESS(status)) {
            Log(L"Cursor query failed", status);
            return false;
        }
        if (output.IsCursorShapeUpdated) lastShape_ = output.CursorShapeInfo.ShapeId;
        return true;
    }
    HRESULT Process() noexcept {
        ComPtr<IDXGIFactory4> factory;
        HRESULT hr = CreateDXGIFactory2(0, IID_PPV_ARGS(&factory));
        if (FAILED(hr)) return hr;
        ComPtr<IDXGIAdapter> adapter;
        hr = factory->EnumAdapterByLuid(luid_, IID_PPV_ARGS(&adapter));
        if (FAILED(hr)) return hr;
        ComPtr<ID3D11Device> device;
        hr = D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
            &device, nullptr, nullptr);
        if (FAILED(hr)) return hr;
        ComPtr<IDXGIDevice> dxgi;
        hr = device.As(&dxgi);
        if (FAILED(hr)) return hr;
        IDARG_IN_SWAPCHAINSETDEVICE binding{};
        binding.pDevice = dxgi.Get();
        hr = IddCxSwapChainSetDevice(chain_, &binding);
        if (FAILED(hr)) return hr;

        const HANDLE events[] = {stop_.Get(), cursorEvent_, frameEvent_};
        for (;;) {
            // Check stop and cursor even under continuous frame delivery.
            if (WaitForSingleObject(stop_.Get(), 0) == WAIT_OBJECT_0) return S_OK;
            if (WaitForSingleObject(cursorEvent_, 0) == WAIT_OBJECT_0 && !ReadCursor())
                return E_FAIL;
            IDARG_OUT_RELEASEANDACQUIREBUFFER next{};
            hr = IddCxSwapChainReleaseAndAcquireBuffer(chain_, &next);
            if (hr == E_PENDING) {
                const DWORD event = WaitForMultipleObjects(3, events, FALSE, 16);
                if (event == WAIT_OBJECT_0) return S_OK;
                if (event == WAIT_OBJECT_0 + 1 && !ReadCursor()) return E_FAIL;
                if (event == WAIT_FAILED) return HRESULT_FROM_WIN32(GetLastError());
                if (event != WAIT_TIMEOUT && event != WAIT_OBJECT_0 + 1 && event != WAIT_OBJECT_0 + 2)
                    return E_UNEXPECTED;
                hr = device->GetDeviceRemovedReason();
                if (FAILED(hr)) return hr;
                continue;
            }
            if (FAILED(hr)) return hr;
            ComPtr<IDXGIResource> surface;
            surface.Attach(next.MetaData.pSurface);
            // Desktop Duplication owns capture; promptly acknowledge every desktop frame.
            surface.Reset();
            hr = IddCxSwapChainFinishedProcessingFrame(chain_);
            if (FAILED(hr)) return hr;
        }
    }
    static DWORD WINAPI ThreadMain(void* argument) noexcept {
        auto* self = static_cast<FramePump*>(argument);
        DWORD task = 0;
        HANDLE scheduling = AvSetMmThreadCharacteristicsW(L"Distribution", &task);
        if (!scheduling) Log(L"MMCSS registration failed", GetLastError());
        const HRESULT hr = self->Process();
        if (FAILED(hr)) Log(L"Swapchain processing ended; requesting replacement", hr);
        // Returning ownership tells IddCx to reassign after loss or processing failure.
        WdfObjectDelete(self->chain_);
        self->chain_ = nullptr;
        if (scheduling) AvRevertMmThreadCharacteristics(scheduling);
        return 0;
    }
public:
    FramePump(IDDCX_MONITOR monitor, const IDARG_IN_SETSWAPCHAIN& args, HANDLE cursor) noexcept
        : monitor_(monitor), chain_(args.hSwapChain), luid_(args.RenderAdapterLuid),
          frameEvent_(args.hNextSurfaceAvailable), cursorEvent_(cursor) {}
    NTSTATUS Start() noexcept {
        stop_.Reset(CreateEventW(nullptr, TRUE, FALSE, nullptr));
        if (!stop_.Get()) return STATUS_INSUFFICIENT_RESOURCES;
        thread_.Reset(CreateThread(nullptr, 0, ThreadMain, this, 0, nullptr));
        return thread_.Get() ? STATUS_SUCCESS : STATUS_INSUFFICIENT_RESOURCES;
    }
    ~FramePump() {
        if (thread_.Get()) {
            SetEvent(stop_.Get());
            WaitForSingleObject(thread_.Get(), INFINITE);
        }
        // On Start failure the caller returns ABANDON_SWAPCHAIN; IddCx retains ownership.
    }
};

static void StopMonitor(IDDCX_MONITOR monitor, bool closing) noexcept {
    auto* state = GetMonitorState(monitor);
    Lock lock(state->pumpLock);
    state->closing = state->closing || closing;
    delete state->pump;
    state->pump = nullptr;
}

_Use_decl_annotations_
void MonitorCleanup(WDFOBJECT object) {
    auto* monitor = GetMonitorState(object);
    StopMonitor(static_cast<IDDCX_MONITOR>(object), true);
    auto* registry = Registry();
    Lock lock(registry->registryLock);
    if (monitor->registered) {
        MonitorState** entry = &registry->monitors;
        while (*entry && *entry != monitor) entry = &(*entry)->next;
        if (*entry) *entry = monitor->next;
        monitor->registered = false;
    }
}
_Use_decl_annotations_
void MonitorDestroy(WDFOBJECT object) {
    auto* monitor = GetMonitorState(object);
    if (monitor->cursorEvent) CloseHandle(monitor->cursorEvent);
}

static DISPLAYCONFIG_VIDEO_SIGNAL_INFO Signal(Mode mode, bool target) noexcept {
    DISPLAYCONFIG_VIDEO_SIGNAL_INFO info{};
    info.activeSize = {mode.width, mode.height};
    info.totalSize = {mode.width + sunna::HBlank, mode.height + sunna::VBlank};
    info.pixelRate = sunna::PixelRate(mode);
    info.vSyncFreq = {mode.hz, 1};
    info.hSyncFreq = {(mode.height + sunna::VBlank) * mode.hz, 1};
    info.AdditionalSignalInfo.videoStandard = 255;
    info.AdditionalSignalInfo.vSyncFreqDivider = target ? 1 : 0;
    info.scanLineOrdering = DISPLAYCONFIG_SCANLINE_ORDERING_PROGRESSIVE;
    return info;
}
static NTSTATUS CopyMonitorModes(const Modes& modes, UINT capacity, IDDCX_MONITOR_MODE* output) noexcept {
    if (!capacity) return STATUS_SUCCESS;
    if (capacity < modes.count) return STATUS_BUFFER_TOO_SMALL;
    if (!output) return STATUS_INVALID_PARAMETER;
    for (UINT i = 0; i < modes.count; ++i) {
        output[i] = {};
        output[i].Size = sizeof(output[i]);
        output[i].Origin = IDDCX_MONITOR_MODE_ORIGIN_DRIVER;
        output[i].MonitorVideoSignalInfo = Signal(modes.items[i], false);
    }
    return STATUS_SUCCESS;
}

_Use_decl_annotations_
NTSTATUS ParseDescription(const IDARG_IN_PARSEMONITORDESCRIPTION* input,
                          IDARG_OUT_PARSEMONITORDESCRIPTION* output) {
    if (input->MonitorDescription.Type != IDDCX_MONITOR_DESCRIPTION_TYPE_EDID ||
        input->MonitorDescription.DataSize != 128 || !input->MonitorDescription.pData)
        return STATUS_INVALID_PARAMETER;
    const BYTE* edid = static_cast<const BYTE*>(input->MonitorDescription.pData);
    const UINT id = UINT(edid[12]) | (UINT(edid[13]) << 8) | (UINT(edid[14]) << 16) | (UINT(edid[15]) << 24);
    Modes modes{};
    bool found = false;
    {
        auto* registry = Registry();
        Lock lock(registry->registryLock);
        for (auto* monitor = registry->monitors; monitor; monitor = monitor->next) {
            if (monitor->id == id && memcmp(monitor->edid, edid, 128) == 0) {
                modes = monitor->modes;
                found = true;
                break;
            }
        }
    }
    if (!found) return STATUS_INVALID_PARAMETER;
    output->MonitorModeBufferOutputCount = modes.count;
    output->PreferredMonitorModeIdx = 0;
    return CopyMonitorModes(modes, input->MonitorModeBufferInputCount, input->pMonitorModes);
}
_Use_decl_annotations_
NTSTATUS DefaultModes(IDDCX_MONITOR monitor, const IDARG_IN_GETDEFAULTDESCRIPTIONMODES* input,
                      IDARG_OUT_GETDEFAULTDESCRIPTIONMODES* output) {
    const auto& modes = GetMonitorState(monitor)->modes;
    output->DefaultMonitorModeBufferOutputCount = modes.count;
    output->PreferredMonitorModeIdx = 0;
    return CopyMonitorModes(modes, input->DefaultMonitorModeBufferInputCount, input->pDefaultMonitorModes);
}
_Use_decl_annotations_
NTSTATUS TargetModes(IDDCX_MONITOR monitor, const IDARG_IN_QUERYTARGETMODES* input,
                     IDARG_OUT_QUERYTARGETMODES* output) {
    const auto& modes = GetMonitorState(monitor)->modes;
    output->TargetModeBufferOutputCount = modes.count;
    if (!input->TargetModeBufferInputCount) return STATUS_SUCCESS;
    if (input->TargetModeBufferInputCount < modes.count) return STATUS_BUFFER_TOO_SMALL;
    if (!input->pTargetModes) return STATUS_INVALID_PARAMETER;
    for (UINT i = 0; i < modes.count; ++i) {
        auto& target = input->pTargetModes[i];
        target = {};
        target.Size = sizeof(target);
        target.TargetVideoSignalInfo.targetVideoSignalInfo = Signal(modes.items[i], true);
    }
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
NTSTATUS CommitModes(IDDCX_ADAPTER, const IDARG_IN_COMMITMODES*) { return STATUS_SUCCESS; }

_Use_decl_annotations_
NTSTATUS AssignSwapChain(IDDCX_MONITOR monitor, const IDARG_IN_SETSWAPCHAIN* input) {
    auto* state = GetMonitorState(monitor);
    Lock lock(state->pumpLock);
    delete state->pump;
    state->pump = nullptr;
    if (state->closing) {
        return STATUS_GRAPHICS_INDIRECT_DISPLAY_ABANDON_SWAPCHAIN;
    }
    IDARG_IN_SETUP_HWCURSOR cursor{};
    cursor.CursorInfo.Size = sizeof(cursor.CursorInfo);
    cursor.CursorInfo.AlphaCursorSupport = TRUE;
    cursor.CursorInfo.ColorXorCursorSupport = IDDCX_XOR_CURSOR_SUPPORT_FULL;
    cursor.CursorInfo.MaxX = cursor.CursorInfo.MaxY = 256;
    cursor.hNewCursorDataAvailable = state->cursorEvent;
    NTSTATUS status = IddCxMonitorSetupHardwareCursor(monitor, &cursor);
    if (!NT_SUCCESS(status)) {
        Log(L"Hardware cursor setup failed", status);
        return STATUS_GRAPHICS_INDIRECT_DISPLAY_ABANDON_SWAPCHAIN;
    }
    state->pump = new (std::nothrow) FramePump(monitor, *input, state->cursorEvent);
    if (!state->pump) {
        Log(L"Cannot allocate frame pump", static_cast<ULONG>(STATUS_INSUFFICIENT_RESOURCES));
        return STATUS_GRAPHICS_INDIRECT_DISPLAY_ABANDON_SWAPCHAIN;
    }
    status = state->pump->Start();
    if (!NT_SUCCESS(status)) {
        Log(L"Cannot start frame pump", status);
        delete state->pump;
        state->pump = nullptr;
        return STATUS_GRAPHICS_INDIRECT_DISPLAY_ABANDON_SWAPCHAIN;
    }
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
NTSTATUS UnassignSwapChain(IDDCX_MONITOR monitor) {
    StopMonitor(monitor, false);
    return STATUS_SUCCESS;
}

static NTSTATUS AddMonitor(DeviceState* device, Mode requested, UINT* id) noexcept {
    if (!device->powered || !device->adapter ||
        InterlockedCompareExchange(&GetAdapterState(device->adapter)->ready, 0, 0) != 1)
        return STATUS_DEVICE_NOT_READY;
    UINT slot = 0;
    while (slot < SUNNA_DISPLAY_MAX_MONITORS && device->monitors[slot]) ++slot;
    if (slot == SUNNA_DISPLAY_MAX_MONITORS) return STATUS_INSUFFICIENT_RESOURCES;
    UINT serial;
    {
        auto* registry = Registry();
        Lock lock(registry->registryLock);
        // Never recycle an ID within a driver-host lifetime (including failed arrivals).
        if (registry->nextId == UINT_MAX) return STATUS_INTEGER_OVERFLOW;
        serial = ++registry->nextId;
    }
    BYTE edid[128];
    sunna::MakeEdid(edid, serial, requested);
    IDDCX_MONITOR_INFO info{};
    info.Size = sizeof(info);
    info.MonitorType = DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EXTERNAL;
    info.ConnectorIndex = slot;
    info.MonitorContainerId = {0x7e7ba8c2, 0x105a, 0x4ba9, {0xa9,0x83,0x53,0x4e,0,0,0,0}};
    for (UINT i = 0; i < 4; ++i) info.MonitorContainerId.Data4[4+i] = BYTE(serial >> (8*i));
    info.MonitorDescription.Size = sizeof(info.MonitorDescription);
    info.MonitorDescription.Type = IDDCX_MONITOR_DESCRIPTION_TYPE_EDID;
    info.MonitorDescription.DataSize = sizeof(edid);
    info.MonitorDescription.pData = edid;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, MonitorState);
    attributes.EvtCleanupCallback = MonitorCleanup;
    attributes.EvtDestroyCallback = MonitorDestroy;
    IDARG_IN_MONITORCREATE create{};
    create.ObjectAttributes = &attributes;
    create.pMonitorInfo = &info;
    IDARG_OUT_MONITORCREATE created{};
    NTSTATUS status = IddCxMonitorCreate(device->adapter, &create, &created);
    if (!NT_SUCCESS(status)) return status;
    auto* state = GetMonitorState(created.MonitorObject);
    state->object = created.MonitorObject;
    state->id = serial;
    state->modes = sunna::MakeModes(requested);
    memcpy(state->edid, edid, sizeof(edid));
    state->cursorEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!state->cursorEvent) {
        WdfObjectDelete(created.MonitorObject);
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    {
        auto* registry = Registry();
        Lock lock(registry->registryLock);
        state->next = registry->monitors;
        registry->monitors = state;
        state->registered = true;
    }
    IDARG_OUT_MONITORARRIVAL arrived{};
    status = IddCxMonitorArrival(created.MonitorObject, &arrived);
    if (!NT_SUCCESS(status)) {
        WdfObjectDelete(created.MonitorObject);
        return status;
    }
    WdfObjectReference(created.MonitorObject);
    device->monitors[slot] = created.MonitorObject;
    *id = serial;
    Log(L"Monitor arrived", serial);
    return STATUS_SUCCESS;
}
static NTSTATUS RemoveMonitor(DeviceState* device, UINT id) noexcept {
    for (auto& monitor : device->monitors) {
        if (monitor && GetMonitorState(monitor)->id == id) {
            StopMonitor(monitor, true);
            const NTSTATUS status = IddCxMonitorDeparture(monitor);
            if (!NT_SUCCESS(status)) {
                auto* state = GetMonitorState(monitor);
                Lock lock(state->pumpLock);
                state->closing = false;
                return status;
            }
            // Departure deletes the object; release only our explicit reference.
            WdfObjectDereference(monitor);
            monitor = nullptr;
            Log(L"Monitor departed", id);
            return STATUS_SUCCESS;
        }
    }
    return STATUS_NOT_FOUND;
}

_Use_decl_annotations_
void DeviceControl(WDFDEVICE device, WDFREQUEST request, size_t outputLength,
                   size_t inputLength, ULONG code) {
    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;
    ULONG_PTR written = 0;
    auto* state = GetDeviceState(device);
    {
        Lock lock(state->operationLock);
        if (code == IOCTL_SUNNA_DISPLAY_INFO) {
            SUNNA_DISPLAY_INFO* output = nullptr;
            status = inputLength ? STATUS_INVALID_PARAMETER :
                WdfRequestRetrieveOutputBuffer(request, sizeof(*output), reinterpret_cast<void**>(&output), nullptr);
            if (NT_SUCCESS(status)) {
                *output = {SUNNA_DISPLAY_VERSION, SUNNA_DISPLAY_MAX_MONITORS};
                written = sizeof(*output);
            }
        } else if (code == IOCTL_SUNNA_DISPLAY_ADD) {
            SUNNA_DISPLAY_ADD* input = nullptr;
            SUNNA_DISPLAY_ID* output = nullptr;
            status = inputLength == sizeof(*input) ?
                WdfRequestRetrieveInputBuffer(request, sizeof(*input), reinterpret_cast<void**>(&input), nullptr) :
                STATUS_INVALID_BUFFER_SIZE;
            if (NT_SUCCESS(status)) {
                // METHOD_BUFFERED aliases input and output; take a value copy before writing.
                const SUNNA_DISPLAY_ADD add = *input;
                status = WdfRequestRetrieveOutputBuffer(request, sizeof(*output), reinterpret_cast<void**>(&output), nullptr);
                if (NT_SUCCESS(status)) {
                    const Mode mode{add.width, add.height, add.refresh_hz};
                    status = add.version == SUNNA_DISPLAY_VERSION && sunna::Valid(mode) ?
                        AddMonitor(state, mode, &output->id) : STATUS_INVALID_PARAMETER;
                    if (NT_SUCCESS(status)) written = sizeof(*output);
                }
            }
        } else if (code == IOCTL_SUNNA_DISPLAY_REMOVE) {
            SUNNA_DISPLAY_ID* input = nullptr;
            status = inputLength == sizeof(*input) && outputLength == 0 ?
                WdfRequestRetrieveInputBuffer(request, sizeof(*input), reinterpret_cast<void**>(&input), nullptr) :
                STATUS_INVALID_BUFFER_SIZE;
            if (NT_SUCCESS(status)) status = state->powered ? RemoveMonitor(state, input->id) : STATUS_DEVICE_NOT_READY;
        }
    }
    WdfRequestCompleteWithInformation(request, status, written);
}

_Use_decl_annotations_
NTSTATUS AdapterReady(IDDCX_ADAPTER adapter, const IDARG_IN_ADAPTER_INIT_FINISHED* input) {
    InterlockedExchange(&GetAdapterState(adapter)->ready, NT_SUCCESS(input->AdapterInitStatus) ? 1 : -1);
    Log(L"Adapter initialization completed", input->AdapterInitStatus);
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
NTSTATUS EnterD0(WDFDEVICE device, WDF_POWER_DEVICE_STATE) {
    auto* state = GetDeviceState(device);
    Lock lock(state->operationLock);
    state->powered = true;
    if (state->adapter) return STATUS_SUCCESS;
    IDDCX_ENDPOINT_VERSION version{};
    version.Size = sizeof(version);
    version.MajorVer = 1;
    IDDCX_ADAPTER_CAPS caps{};
    caps.Size = sizeof(caps);
    caps.MaxMonitorsSupported = SUNNA_DISPLAY_MAX_MONITORS;
    caps.EndPointDiagnostics.Size = sizeof(caps.EndPointDiagnostics);
    caps.EndPointDiagnostics.GammaSupport = IDDCX_FEATURE_IMPLEMENTATION_NONE;
    caps.EndPointDiagnostics.TransmissionType = IDDCX_TRANSMISSION_TYPE_WIRED_OTHER;
    caps.EndPointDiagnostics.pEndPointFriendlyName = L"Sunna";
    caps.EndPointDiagnostics.pEndPointManufacturerName = L"Sunna";
    caps.EndPointDiagnostics.pEndPointModelName = L"Sunna Display";
    caps.EndPointDiagnostics.pHardwareVersion = &version;
    caps.EndPointDiagnostics.pFirmwareVersion = &version;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, AdapterState);
    IDARG_IN_ADAPTER_INIT input{};
    input.WdfDevice = device;
    input.pCaps = &caps;
    input.ObjectAttributes = &attributes;
    IDARG_OUT_ADAPTER_INIT output{};
    const NTSTATUS status = IddCxAdapterInitAsync(&input, &output);
    if (NT_SUCCESS(status)) {
        state->adapter = output.AdapterObject;
        WdfObjectReference(state->adapter);
    } else {
        state->powered = false;
        Log(L"Adapter initialization could not start", status);
    }
    return status;
}
_Use_decl_annotations_
NTSTATUS LeaveD0(WDFDEVICE device, WDF_POWER_DEVICE_STATE) {
    auto* state = GetDeviceState(device);
    Lock lock(state->operationLock);
    state->powered = false;
    for (auto monitor : state->monitors) if (monitor) StopMonitor(monitor, false);
    return STATUS_SUCCESS;
}
static void ReleaseReferences(DeviceState* state) noexcept {
    state->powered = false;
    for (auto& monitor : state->monitors) {
        if (monitor) {
            StopMonitor(monitor, true);
            WdfObjectDereference(monitor);
            monitor = nullptr;
        }
    }
    if (state->adapter) {
        WdfObjectDereference(state->adapter);
        state->adapter = nullptr;
    }
}
_Use_decl_annotations_
NTSTATUS ReleaseHardware(WDFDEVICE device, WDFCMRESLIST) {
    auto* state = GetDeviceState(device);
    Lock lock(state->operationLock);
    // IddCx owns deletion when the PnP device stops. A subsequent start gets a new adapter.
    ReleaseReferences(state);
    return STATUS_SUCCESS;
}
_Use_decl_annotations_
void DeviceCleanup(WDFOBJECT object) {
    auto* state = GetDeviceState(object);
    Lock lock(state->operationLock);
    ReleaseReferences(state);
}
_Use_decl_annotations_
NTSTATUS DeviceAdd(WDFDRIVER, PWDFDEVICE_INIT init) {
    WDF_PNPPOWER_EVENT_CALLBACKS power;
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&power);
    power.EvtDeviceD0Entry = EnterD0;
    power.EvtDeviceD0Exit = LeaveD0;
    power.EvtDeviceReleaseHardware = ReleaseHardware;
    WdfDeviceInitSetPnpPowerEventCallbacks(init, &power);
    IDD_CX_CLIENT_CONFIG callbacks;
    IDD_CX_CLIENT_CONFIG_INIT(&callbacks);
    callbacks.EvtIddCxDeviceIoControl = DeviceControl;
    callbacks.EvtIddCxAdapterInitFinished = AdapterReady;
    callbacks.EvtIddCxAdapterCommitModes = CommitModes;
    callbacks.EvtIddCxParseMonitorDescription = ParseDescription;
    callbacks.EvtIddCxMonitorGetDefaultDescriptionModes = DefaultModes;
    callbacks.EvtIddCxMonitorQueryTargetModes = TargetModes;
    callbacks.EvtIddCxMonitorAssignSwapChain = AssignSwapChain;
    callbacks.EvtIddCxMonitorUnassignSwapChain = UnassignSwapChain;
    NTSTATUS status = IddCxDeviceInitConfig(init, &callbacks);
    if (!NT_SUCCESS(status)) return status;
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DeviceState);
    attributes.EvtCleanupCallback = DeviceCleanup;
    attributes.ExecutionLevel = WdfExecutionLevelPassive;
    attributes.SynchronizationScope = WdfSynchronizationScopeNone;
    WDFDEVICE device;
    status = WdfDeviceCreate(&init, &attributes, &device);
    if (!NT_SUCCESS(status)) return status;
    status = IddCxDeviceInitialize(device);
    if (!NT_SUCCESS(status)) return status;
    return WdfDeviceCreateDeviceInterface(device, &InterfaceId, nullptr);
}
_Use_decl_annotations_
extern "C" NTSTATUS DriverEntry(PDRIVER_OBJECT driver, PUNICODE_STRING path) {
    WDF_DRIVER_CONFIG config;
    WDF_DRIVER_CONFIG_INIT(&config, DeviceAdd);
    WDF_OBJECT_ATTRIBUTES attributes;
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&attributes, DriverState);
    return WdfDriverCreate(driver, path, &attributes, &config, WDF_NO_HANDLE);
}
