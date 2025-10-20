
@preconcurrency import Foundation
@preconcurrency import FSKit
import os
import Darwin

@_silgen_name("af_register_process")
func af_register_process(_ fs: UInt64, _ pid: UInt32, _ parent_pid: UInt32, _ uid: UInt32, _ gid: UInt32, _ out_pid: UnsafeMutablePointer<UInt32>?) -> Int32

@_silgen_name("agentfs_bridge_statfs")
func agentfs_bridge_statfs(_ core: UnsafeMutableRawPointer?, _ buffer: UnsafeMutablePointer<CChar>?, _ buffer_size: size_t) -> Int32

@_silgen_name("af_getattr")
func af_getattr(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ buffer: UnsafeMutablePointer<CChar>?, _ buffer_size: size_t) -> Int32

@_silgen_name("af_stats")
func af_stats(_ fs: UInt64, _ out_stats: UnsafeMutablePointer<UInt8>?, _ stats_size: size_t) -> Int32

@_silgen_name("af_mkdir")
func af_mkdir(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ mode: UInt32) -> Int32

@_silgen_name("af_readdir")
func af_readdir(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ buffer: UnsafeMutablePointer<CChar>?, _ buffer_size: size_t, _ out_len: UnsafeMutablePointer<size_t>?) -> Int32

@_silgen_name("af_open")
func af_open(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ options: UnsafePointer<CChar>?, _ handle: UnsafeMutablePointer<UInt64>?) -> Int32

@_silgen_name("af_open_by_id")
func af_open_by_id(_ fs: UInt64, _ pid: UInt32, _ node_id: UInt64, _ options: UnsafePointer<CChar>?, _ handle: UnsafeMutablePointer<UInt64>?) -> Int32

@_silgen_name("af_read")
func af_read(_ fs: UInt64, _ pid: UInt32, _ handle: UInt64, _ offset: UInt64, _ buffer: UnsafeMutableRawPointer?, _ length: UInt32, _ bytes_read: UnsafeMutablePointer<UInt32>?) -> Int32

@_silgen_name("af_write")
func af_write(_ fs: UInt64, _ pid: UInt32, _ handle: UInt64, _ offset: UInt64, _ buffer: UnsafeRawPointer?, _ length: UInt32, _ bytes_written: UnsafeMutablePointer<UInt32>?) -> Int32

@_silgen_name("af_close")
func af_close(_ fs: UInt64, _ pid: UInt32, _ handle: UInt64) -> Int32

@_silgen_name("af_symlink")
func af_symlink(_ fs: UInt64, _ pid: UInt32, _ target: UnsafePointer<CChar>?, _ linkpath: UnsafePointer<CChar>?) -> Int32

@_silgen_name("af_readlink")
func af_readlink(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ buffer: UnsafeMutablePointer<CChar>?, _ buffer_size: size_t) -> Int32

@_silgen_name("af_rename")
func af_rename(_ fs: UInt64, _ pid: UInt32, _ oldpath: UnsafePointer<CChar>?, _ newpath: UnsafePointer<CChar>?) -> Int32

@_silgen_name("af_rmdir")
func af_rmdir(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?) -> Int32

@_silgen_name("af_unlink")
func af_unlink(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?) -> Int32

@_silgen_name("af_set_times")
func af_set_times(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ atime: Int64, _ mtime: Int64, _ ctime: Int64, _ birthtime: Int64) -> Int32

@_silgen_name("af_set_mode")
func af_set_mode(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ mode: UInt32) -> Int32

@_silgen_name("af_set_owner")
func af_set_owner(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ uid: UInt32, _ gid: UInt32) -> Int32

@_silgen_name("af_xattr_get")
func af_xattr_get(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ name: UnsafePointer<CChar>?, _ buffer: UnsafeMutableRawPointer?, _ buffer_size: size_t, _ out_len: UnsafeMutablePointer<size_t>?) -> Int32

@_silgen_name("af_xattr_set")
func af_xattr_set(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ name: UnsafePointer<CChar>?, _ value: UnsafeRawPointer?, _ value_len: size_t) -> Int32

@_silgen_name("af_xattr_list")
func af_xattr_list(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ buffer: UnsafeMutableRawPointer?, _ buffer_size: size_t, _ out_len: UnsafeMutablePointer<size_t>?) -> Int32

@_silgen_name("af_resolve_id")
func af_resolve_id(_ fs: UInt64, _ pid: UInt32, _ path: UnsafePointer<CChar>?, _ node_id: UnsafeMutablePointer<UInt64>?, _ parent_id: UnsafeMutablePointer<UInt64>?) -> Int32

@_silgen_name("af_create_child_by_id")
func af_create_child_by_id(_ fs: UInt64, _ parent_id: UInt64, _ name_ptr: UnsafePointer<UInt8>?, _ name_len: Int, _ item_type: UInt32, _ mode: UInt32, _ out_node_id: UnsafeMutablePointer<UInt64>?) -> Int32

@available(macOS 15.4, *)
private struct ProcessIdentityKey: Hashable {
    let pid: pid_t
    let pidVersion: UInt32
}

@available(macOS 15.4, *)
private struct ProcessIdentity {
    let pid: pid_t
    let pidVersion: UInt32
    let uid: uid_t
    let gid: gid_t
}

@available(macOS 15.4, *)
final class AgentFsVolume: FSVolume {

    // MARK: - Byte helpers

    /// Call body with a NUL-terminated C string created from a Data buffer.
    @inline(__always)
    private func withNullTerminatedCStr<R>(_ bytes: Data, _ body: (UnsafePointer<CChar>) -> R) -> R {
        var tmp = bytes
        tmp.append(0)
        return tmp.withUnsafeBytes { raw in
            let ptr = raw.bindMemory(to: CChar.self).baseAddress!
            return body(ptr)
        }
    }

    /// Join directory bytes with the provided child name without materializing Swift Strings.
    @inline(__always)
    private func constructPathBytes(for name: FSFileName, in directory: AgentFsItem) -> Data {
        var output = directory.pathBytes
        if output.last != 0x2f { // '/'
            output.append(0x2f)
        }
        output.append(contentsOf: name.data)
        return output
    }

    private func obtainHandle(for item: AgentFsItem, wantsWrite: Bool) throws -> (handle: UInt64, pid: UInt32, transient: Bool) {
        if let existing = handleStateQueue.sync(execute: { () -> (UInt64, UInt32?)? in
            guard let handle = opensByItem[item.attributes.fileID]?.last else { return nil }
            return (handle, handleToPid[handle])
        }) {
            let pid = existing.1 ?? getCallingPid()
            return (existing.0, pid, false)
        }

        var handle: UInt64 = 0
        let pid = getCallingPid()
        let optionsJson = "{\"read\":true,\"write\":\(wantsWrite)}"
        let result = coreQueue.sync {
            optionsJson.withCString { options_cstr in
                af_open_by_id(fsHandle, pid, item.attributes.fileID.rawValue, options_cstr, &handle)
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            }
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        return (handle, pid, true)
    }

    /// Convert AfResult error codes to FSKit errors
    /// Returns nil for success (AfOk = 0), or an Error for actual errors
    private func afResultToFSKitError(_ result: Int32) -> Error? {
        switch result {
        case 0: // AfOk - success, no error
            return nil
        case 2: // AfErrNotFound -> ENOENT
            return fs_errorForPOSIXError(POSIXError.ENOENT.rawValue)
        case 17: // AfErrExists -> EEXIST
            return fs_errorForPOSIXError(POSIXError.EEXIST.rawValue)
        case 13: // AfErrAcces -> EACCES
            return fs_errorForPOSIXError(POSIXError.EACCES.rawValue)
        case 28: // AfErrNospc -> ENOSPC
            return fs_errorForPOSIXError(POSIXError.ENOSPC.rawValue)
        case 22: // AfErrInval -> EINVAL
            return fs_errorForPOSIXError(POSIXError.EINVAL.rawValue)
        case 16: // AfErrBusy -> EBUSY
            return fs_errorForPOSIXError(POSIXError.EBUSY.rawValue)
        case 95: // AfErrUnsupported -> ENOTSUP
            return fs_errorForPOSIXError(POSIXError.ENOTSUP.rawValue)
        default:
            return fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }
    }

    private let resource: FSResource
    private let coreHandle: UnsafeMutableRawPointer?
    private let coreQueue = DispatchQueue(label: "com.agentfs.AgentFSKitExtension.core")

    /// Generate unique item IDs using the shared generator
    private static func generateItemID() -> UInt64 {
        return AgentFsItem.generateUniqueItemID()
    }

    private let logger = Logger(subsystem: "com.agentfs.AgentFSKitExtension", category: "AgentFsVolume")

    private let root: AgentFsItem
    private var processCache: [ProcessIdentityKey: UInt32] = [:] // caller identity -> registered PID
    private var handleToPid: [UInt64: UInt32] = [:] // Map from handle ID to registered PID
    private var opensByItem: [FSItem.Identifier: [UInt64]] = [:]
    private let handleStateQueue = DispatchQueue(label: "com.agentfs.AgentFSKitExtension.handleState")

    private static let auditTokenClass: NSObject.Type? = NSClassFromString("FSAuditToken") as? NSObject.Type
    private static let selToken = NSSelectorFromString("token")
    private static let selPid = NSSelectorFromString("pid")
    private static let selPidVersion = NSSelectorFromString("pidversion")
    private static let selEuid = NSSelectorFromString("euid")
    private static let selEgid = NSSelectorFromString("egid")

    private var fsHandle: UInt64 {
        return coreHandle?.load(as: UInt64.self) ?? 0
    }

    init(resource: FSResource, coreHandle: UnsafeMutableRawPointer?) {
        self.resource = resource
        self.coreHandle = coreHandle

        // Create root item with fixed attributes
        self.root = AgentFsItem.createRoot()

        super.init(
            volumeID: FSVolume.Identifier(uuid: Constants.volumeIdentifier),
            volumeName: FSFileName(string: "AgentFS")
        )
    }

    /// Attempt to resolve the caller identity from FSKit's thread-local audit token.
    private func resolveAuditTokenIdentity() -> ProcessIdentity? {
        guard
            let cls = AgentFsVolume.auditTokenClass,
            let tokenUnmanaged = cls.perform(AgentFsVolume.selToken),
            let token = tokenUnmanaged.takeUnretainedValue() as? NSObject,
            let pidValue = token.perform(AgentFsVolume.selPid)?.takeUnretainedValue() as? NSNumber,
            let pidVersionValue = token.perform(AgentFsVolume.selPidVersion)?.takeUnretainedValue() as? NSNumber,
            let uidValue = token.perform(AgentFsVolume.selEuid)?.takeUnretainedValue() as? NSNumber,
            let gidValue = token.perform(AgentFsVolume.selEgid)?.takeUnretainedValue() as? NSNumber
        else {
            return nil
        }

        return ProcessIdentity(
            pid: pid_t(pidValue.int32Value),
            pidVersion: pidVersionValue.uint32Value,
            uid: uid_t(uidValue.uint32Value),
            gid: gid_t(gidValue.uint32Value)
        )
    }

    /// Get the calling process identity, falling back to the extension process when unavailable.
    private func getCallingProcessIdentity() -> ProcessIdentity {
        if let identity = resolveAuditTokenIdentity() {
            return identity
        }
        return ProcessIdentity(pid: getpid(), pidVersion: 0, uid: getuid(), gid: getgid())
    }

    /// Get or register a process and return the registered PID.
    private func getRegisteredPid(for identity: ProcessIdentity) -> UInt32 {
        let key = ProcessIdentityKey(pid: identity.pid, pidVersion: identity.pidVersion)
        if let cached = processCache[key] {
            return cached
        }

        var registeredPid: UInt32 = 0
        let result = af_register_process(
            fsHandle,
            UInt32(identity.pid),
            0,
            UInt32(identity.uid),
            UInt32(identity.gid),
            &registeredPid
        )

        if result == 0 {
            processCache[key] = registeredPid
            logger.debug("Registered PID \(identity.pid)@\(identity.pidVersion) as \(registeredPid) uid=\(identity.uid) gid=\(identity.gid)")
            return registeredPid
        } else {
            logger.error("Failed to register PID \(identity.pid)@\(identity.pidVersion); result=\(result)")
            let fallback = UInt32(identity.pid)
            processCache[key] = fallback
            return fallback
        }
    }

    /// Get the registered PID for a handle.
    private func getPidForHandle(_ handle: UInt64) -> UInt32 {
        if let pid = handleStateQueue.sync(execute: { handleToPid[handle] }) {
            return pid
        }
        let identity = getCallingProcessIdentity()
        return getRegisteredPid(for: identity)
    }

    /// Get the calling process PID, registering it if necessary.
    private func getCallingPid() -> UInt32 {
        let identity = getCallingProcessIdentity()
        return getRegisteredPid(for: identity)
    }

}

@available(macOS 15.4, *)
extension AgentFsVolume: FSVolume.PathConfOperations {

    var maximumLinkCount: Int { 65_535 }

    var maximumNameLength: Int { 255 }

    var restrictsOwnershipChanges: Bool { false }

    var truncatesLongNames: Bool { false }

    var maximumXattrSize: Int { 65_536 }

    var maximumFileSize: UInt64 { UInt64.max }
}

@available(macOS 15.4, *)
extension AgentFsVolume: FSVolume.Operations {

    var supportedVolumeCapabilities: FSVolume.SupportedCapabilities {
        logger.debug("supportedVolumeCapabilities")

        let capabilities = FSVolume.SupportedCapabilities()

        // Hard links are not implemented in the current AgentFS core
        // TODO: Implement hard link support in AgentFS core when needed
        capabilities.supportsHardLinks = false

        // Symbolic links are fully supported by AgentFS core
        capabilities.supportsSymbolicLinks = true

        // AgentFS is currently an in-memory filesystem, so object IDs are not
        // persistent across filesystem restarts/mounts. This is appropriate for
        // the current use case of temporary, session-based filesystem views.
        capabilities.supportsPersistentObjectIDs = false

        // AgentFS implements volume statistics reporting (total/free blocks, files, etc.)
        // so doesNotSupportVolumeSizes must be false (meaning it DOES support volume sizes)
        capabilities.doesNotSupportVolumeSizes = false

        // AgentFS supports hidden files (files/directories starting with '.')
        capabilities.supportsHiddenFiles = true

        // AgentFS uses 64-bit object IDs for filesystem items, providing
        // sufficient namespace for all practical use cases
        capabilities.supports64BitObjectIDs = true

        return capabilities
    }

    var volumeStatistics: FSStatFSResult {
        logger.debug("volumeStatistics")

        let result = FSStatFSResult(fileSystemTypeName: "AgentFS")

        // Get actual statistics from AgentFS core
        let fsId = coreQueue.sync { () -> UInt64? in
            coreHandle?.load(as: UInt64.self)
        }

        guard let fsId = fsId else {
            logger.warning("volumeStatistics: no core handle available, using defaults")
            // Fallback to reasonable defaults
            result.blockSize = 4096
            result.ioSize = 4096
            result.totalBlocks = 1000000  // 4GB with 4K blocks
            result.availableBlocks = result.totalBlocks
            result.freeBlocks = result.totalBlocks
            result.totalFiles = 100000
            result.freeFiles = 100000
            return result
        }

        var statsBuffer = [UInt8](repeating: 0, count: 28) // 28 bytes for FsStats
        let statsResult = af_stats(fsId, &statsBuffer, statsBuffer.count)

        if statsResult == 0 {
            // Parse FsStats from buffer: branches(u32) + snapshots(u32) + open_handles(u32) + bytes_in_memory(u64) + bytes_spilled(u64)
            var branches: UInt32 = 0
            var snapshots: UInt32 = 0
            var openHandles: UInt32 = 0
            var bytesInMemory: UInt64 = 0
            var bytesSpilled: UInt64 = 0

            statsBuffer.withUnsafeBytes { bufferPtr in
                branches = bufferPtr.load(fromByteOffset: 0, as: UInt32.self)
                snapshots = bufferPtr.load(fromByteOffset: 4, as: UInt32.self)
                openHandles = bufferPtr.load(fromByteOffset: 8, as: UInt32.self)
                bytesInMemory = bufferPtr.load(fromByteOffset: 12, as: UInt64.self)
                bytesSpilled = bufferPtr.load(fromByteOffset: 20, as: UInt64.self)

                logger.debug("AgentFS stats: branches=\(branches), snapshots=\(snapshots), open_handles=\(openHandles), memory=\(bytesInMemory), spilled=\(bytesSpilled)")
            }

            // Convert AgentFS statistics to FSKit format
            result.blockSize = 4096
            result.ioSize = 4096

            // Estimate total space based on memory usage and configuration
            // For AgentFS, we consider total space as memory limit + some spill space
            let memoryLimit: UInt64 = 1024 * 1024 * 1024  // 1GB default, should come from config
            let totalBytes = max(memoryLimit, bytesInMemory + bytesSpilled + 100 * 1024 * 1024) // At least 100MB
            result.totalBlocks = totalBytes / 4096
            result.availableBlocks = (totalBytes - bytesInMemory - bytesSpilled) / 4096
            result.freeBlocks = result.availableBlocks

            // File count based on open handles and estimated capacity
            result.totalFiles = UInt64(max(10000, Int(openHandles) * 10))
            result.freeFiles = result.totalFiles - UInt64(min(Int(result.totalFiles), Int(openHandles)))
        } else {
            logger.warning("Failed to get AgentFS stats, using conservative defaults: error \(statsResult)")
            // Conservative defaults: unknown sizes => report minimal non-zero units
            result.blockSize = 4096
            result.ioSize = 4096
            result.totalBlocks = 0
            result.availableBlocks = 0
            result.freeBlocks = 0
            result.totalFiles = 0
            result.freeFiles = 0
        }

        return result
    }

    func activate(options: FSTaskOptions) async throws -> FSItem {
        logger.debug("activate")
        return root
    }

    func deactivate(options: FSDeactivateOptions = []) async throws {
        logger.debug("deactivate")
    }

    func mount(options: FSTaskOptions) async throws {
        logger.debug("mount")
    }

    func unmount() async {
        logger.debug("unmount")
    }

    func synchronize(flags: FSSyncFlags) async throws {
        logger.debug("synchronize")
    }

    private func fetchAttributesFor(_ agentItem: AgentFsItem) throws -> FSItem.Attributes {
        var buffer = [CChar](repeating: 0, count: 64)
        let ok = coreQueue.sync { () -> Bool in
            let callingPid = getCallingPid()
            return withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                af_getattr(fsHandle, callingPid, cPath, &buffer, buffer.count)
            } == 0
        }
        guard ok else { throw fs_errorForPOSIXError(POSIXError.EIO.rawValue) }

        let size = buffer.withUnsafeBytes { $0.load(fromByteOffset: 0, as: UInt64.self) }
        let fileTypeByte = buffer.withUnsafeBytes { $0.load(fromByteOffset: 8, as: UInt8.self) }
        let mode = buffer.withUnsafeBytes { $0.load(fromByteOffset: 9, as: UInt32.self) }
        let atime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 21, as: Int64.self) }
        let mtime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 29, as: Int64.self) }
        let ctime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 37, as: Int64.self) }
        let birthtime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 45, as: Int64.self) }

        let attrs = FSItem.Attributes()
        switch fileTypeByte {
        case 0: attrs.type = .file
        case 1: attrs.type = .directory
        case 2: attrs.type = .symlink
        default: attrs.type = .file
        }
        attrs.size = size
        attrs.allocSize = size
        attrs.mode = mode
        attrs.parentID = agentItem.attributes.parentID
        attrs.accessTime = timespec(tv_sec: Int(atime), tv_nsec: 0)
        attrs.modifyTime = timespec(tv_sec: Int(mtime), tv_nsec: 0)
        attrs.changeTime = timespec(tv_sec: Int(ctime), tv_nsec: 0)
        attrs.birthTime = timespec(tv_sec: Int(birthtime), tv_nsec: 0)
        return attrs
    }

    func attributes(
        _ desiredAttributes: FSItem.GetAttributesRequest,
        of item: FSItem
    ) async throws -> FSItem.Attributes {
        guard let agentItem = item as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        // Root: return cached
        if agentItem.attributes.fileID == FSItem.Identifier.rootDirectory {
            return agentItem.attributes
        }

        return try fetchAttributesFor(agentItem)
    }

    func setAttributes(
        _ newAttributes: FSItem.SetAttributesRequest,
        on item: FSItem
    ) async throws -> FSItem.Attributes {
        guard let agentItem = item as? AgentFsItem else { throw fs_errorForPOSIXError(POSIXError.EIO.rawValue) }

        // Support mode and times; others unsupported for now
        if newAttributes.isValid(.mode) {
            let mode = newAttributes.mode
            let rc = coreQueue.sync {
                withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                    af_set_mode(fsHandle, getCallingPid(), cPath, mode)
                }
            }
            if rc != 0, let err = afResultToFSKitError(rc) { throw err }
        }

        if newAttributes.isValid(.uid) || newAttributes.isValid(.gid) {
            let uid = newAttributes.uid
            let gid = newAttributes.gid
            let rc = coreQueue.sync {
                withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                    af_set_owner(fsHandle, getCallingPid(), cPath, uid, gid)
                }
            }
            if rc != 0, let err = afResultToFSKitError(rc) { throw err }
        }

        let atime = Int64(newAttributes.accessTime.tv_sec)
        let mtime = Int64(newAttributes.modifyTime.tv_sec)
        let ctime = Int64(newAttributes.changeTime.tv_sec)
        let birthtime = Int64(newAttributes.birthTime.tv_sec)

        var needTimes = false
        if newAttributes.isValid(.accessTime) { needTimes = true }
        if newAttributes.isValid(.modifyTime) { needTimes = true }
        if newAttributes.isValid(.changeTime) { needTimes = true }
        if newAttributes.isValid(.birthTime) { needTimes = true }
        if needTimes {
            let rc = coreQueue.sync {
                withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                    af_set_times(fsHandle, getCallingPid(), cPath, atime, mtime, ctime, birthtime)
                }
            }
            if rc != 0, let err = afResultToFSKitError(rc) { throw err }
        }

        // Return fresh attributes
        return try fetchAttributesFor(agentItem)
    }

    func lookupItem(
        named name: FSFileName,
        inDirectory directory: FSItem
    ) async throws -> (FSItem, FSFileName) {
        logger.debug("lookupItem: \(String(describing: name.string)), \(directory)")

        guard let dirItem = directory as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        let fullPathBytes = constructPathBytes(for: name, in: dirItem)

        var nodeId: UInt64 = 0
        var parentId: UInt64 = 0
        _ = withNullTerminatedCStr(fullPathBytes) { cPath in
            af_resolve_id(fsHandle, getCallingPid(), cPath, &nodeId, &parentId)
        }

        var buffer = [CChar](repeating: 0, count: 64)
        let result = coreQueue.sync { () -> Int32 in
            withNullTerminatedCStr(fullPathBytes) { cPath in
                let callingPid = getCallingPid()
                return af_getattr(fsHandle, callingPid, cPath, &buffer, buffer.count)
            }
        }

        if result != 0 {
            let debugPath = String(decoding: fullPathBytes, as: UTF8.self)
            logger.debug("lookupItem: failed to stat path \(debugPath), error: \(result)")
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        // Parse the attributes from the buffer
        let item = AgentFsItem(name: name, id: nodeId)
        item.pathBytes = fullPathBytes
        item.path = String(decoding: fullPathBytes, as: UTF8.self)
        item.attributes.fileID = FSItem.Identifier(rawValue: nodeId) ?? .invalid
        if parentId != 0, let pid = FSItem.Identifier(rawValue: parentId) {
            item.attributes.parentID = pid
        }

        // Parse attributes from buffer: size(8) + type(1) + mode(4) + times(4x i64)
        if buffer.count >= 48 {
            let size = buffer.withUnsafeBytes { ptr in
                ptr.load(fromByteOffset: 0, as: UInt64.self)
            }
            let fileTypeByte = buffer.withUnsafeBytes { $0.load(fromByteOffset: 8, as: UInt8.self) }
            let mode = buffer.withUnsafeBytes { $0.load(fromByteOffset: 9, as: UInt32.self) }
            let atime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 13, as: Int64.self) }
            let mtime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 21, as: Int64.self) }
            let ctime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 29, as: Int64.self) }
            let birthtime = buffer.withUnsafeBytes { $0.load(fromByteOffset: 37, as: Int64.self) }

            item.attributes.size = size
            item.attributes.allocSize = size
            item.attributes.mode = mode
            item.attributes.accessTime = timespec(tv_sec: Int(atime), tv_nsec: 0)
            item.attributes.modifyTime = timespec(tv_sec: Int(mtime), tv_nsec: 0)
            item.attributes.changeTime = timespec(tv_sec: Int(ctime), tv_nsec: 0)
            item.attributes.birthTime = timespec(tv_sec: Int(birthtime), tv_nsec: 0)

            // Map file type byte to FSItem.ItemType
            switch fileTypeByte {
            case 0: // regular file
                item.attributes.type = .file
            case 1: // directory
                item.attributes.type = .directory
            case 2: // symlink
                item.attributes.type = .symlink
            default:
                item.attributes.type = .file // default fallback
            }
        } else {
            // Fallback if buffer is too small
            item.attributes.type = .file
            item.attributes.size = 0
            item.attributes.allocSize = 0
        }

        // Set the parent ID to link it to the directory
        item.attributes.parentID = dirItem.attributes.fileID

        logger.debug("lookupItem: found item \(name.string ?? "unnamed")")
        return (item, name)
    }

    func reclaimItem(_ item: FSItem) async throws {
        logger.debug("reclaimItem: \(item)")

        guard let agentItem = item as? AgentFsItem else {
            logger.warning("reclaimItem: item is not an AgentFsItem")
            return
        }

        let handlesToClose = handleStateQueue.sync { () -> [(UInt64, UInt32?)] in
            let handles = opensByItem.removeValue(forKey: agentItem.attributes.fileID) ?? []
            return handles.map { handle in
                let pid = handleToPid.removeValue(forKey: handle)
                return (handle, pid)
            }
        }
        agentItem.userData = nil

        for (handle, storedPid) in handlesToClose {
            logger.debug("reclaimItem: closing handle \(handle)")
            let pidForHandle = storedPid ?? getCallingPid()
            let result = coreQueue.sync { af_close(fsHandle, pidForHandle, handle) }
            if result != 0 {
                logger.warning("reclaimItem: failed to close handle \(handle), error: \(result)")
            }
        }

        agentItem.data = nil

        // Note: We don't remove from children here as that's handled by the volume
        // This method is called when FsKit determines the item is no longer needed
        // and can be reclaimed for memory management purposes

        logger.debug("reclaimItem: reclaimed item \(agentItem.name.string ?? "unnamed")")
    }

    func readSymbolicLink(
        _ item: FSItem
    ) async throws -> FSFileName {
        logger.debug("readSymbolicLink: \(item)")

        guard let agentItem = item as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        var buffer = [CChar](repeating: 0, count: 4096)
        let result = coreQueue.sync { () -> Int32 in
            withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                af_readlink(fsHandle, getCallingPid(), cPath, &buffer, buffer.count)
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        // Safely decode C buffer as UTF-8 up to first NUL
        let targetPath: String = {
            let bytes = Data(bytes: buffer, count: strnlen(buffer, buffer.count))
            return String(decoding: bytes, as: UTF8.self)
        }()
        return FSFileName(string: targetPath)
    }

    func createItem(
        named name: FSFileName,
        type: FSItem.ItemType,
        inDirectory directory: FSItem,
        attributes newAttributes: FSItem.SetAttributesRequest
    ) async throws -> (FSItem, FSFileName) {
        logger.debug("createItem: \(String(describing: name.string)) - \(newAttributes.mode)")

        guard let directory = directory as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        // Create using byte-safe API with parent ID
        let parentId = directory.attributes.fileID.rawValue
        var createdNodeId: UInt64 = 0
        let mode = UInt32(newAttributes.mode)
        let itemType: UInt32 = (type == .directory) ? 1 : 0
        let result = coreQueue.sync { () -> Int32 in
            let data = name.data
            return data.withUnsafeBytes { rawPtr in
                let base = rawPtr.bindMemory(to: UInt8.self).baseAddress
                return af_create_child_by_id(fsHandle, parentId, base, data.count, itemType, mode, &createdNodeId)
            }
        }
        if result != 0 {
            if let error = afResultToFSKitError(result) { throw error }
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        // Build AgentFsItem and set path via parent + name bytes
        let item = AgentFsItem(name: name)
        let pathBytes = constructPathBytes(for: name, in: directory)
        item.pathBytes = pathBytes
        item.path = String(decoding: pathBytes, as: UTF8.self)
        mergeAttributes(item.attributes, request: newAttributes)
        item.attributes.parentID = directory.attributes.fileID
        item.attributes.fileID = FSItem.Identifier(rawValue: createdNodeId) ?? .invalid
        item.attributes.type = type
        return (item, name)
    }

    func createSymbolicLink(
        named name: FSFileName,
        inDirectory directory: FSItem,
        attributes newAttributes: FSItem.SetAttributesRequest,
        linkContents contents: FSFileName
    ) async throws -> (FSItem, FSFileName) {
        logger.debug("createSymbolicLink: \(name)")

        guard let directory = directory as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        let linkBytes = constructPathBytes(for: name, in: directory)
        let targetCString = contents.string ?? ""

        let result = coreQueue.sync {
            withNullTerminatedCStr(linkBytes) { link_cstr in
                targetCString.withCString { target_cstr in
                    af_symlink(fsHandle, getCallingPid(), target_cstr, link_cstr)
                }
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        // Create FSItem for the new symlink
        let item = AgentFsItem(name: name)
        item.pathBytes = linkBytes
        item.path = String(decoding: linkBytes, as: UTF8.self)
        mergeAttributes(item.attributes, request: newAttributes)
        item.attributes.parentID = directory.attributes.fileID
        item.attributes.type = .symlink
        // No need to add to in-memory children since we use path-based operations

        return (item, name)
    }

    func createLink(
        to item: FSItem,
        named name: FSFileName,
        inDirectory directory: FSItem
    ) async throws -> FSFileName {
        logger.debug("createLink: \(name)")
        // Hard links are not implemented in the current Rust core
        // TODO: Implement hard link support in Rust core
        throw fs_errorForPOSIXError(POSIXError.ENOTSUP.rawValue)
    }

    func removeItem(
        _ item: FSItem,
        named name: FSFileName,
        fromDirectory directory: FSItem
    ) async throws {
        logger.debug("remove: \(name)")

        guard let agentItem = item as? AgentFsItem, let directory = directory as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        let itemBytes = constructPathBytes(for: name, in: directory)

        let itemType = agentItem.attributes.type
        let result: Int32 = coreQueue.sync {
            withNullTerminatedCStr(itemBytes) { cPath in
                if itemType == .directory {
                    af_rmdir(fsHandle, getCallingPid(), cPath)
                } else {
                    af_unlink(fsHandle, getCallingPid(), cPath)
                }
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        // No need to update in-memory state since we use path-based operations
    }

    func renameItem(
        _ item: FSItem,
        inDirectory sourceDirectory: FSItem,
        named sourceName: FSFileName,
        to destinationName: FSFileName,
        inDirectory destinationDirectory: FSItem,
        overItem: FSItem?
    ) async throws -> FSFileName {
        logger.debug("rename: \(item)")

        guard let agentItem = item as? AgentFsItem,
              let sourceDir = sourceDirectory as? AgentFsItem,
              let destDir = destinationDirectory as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        let sourceBytes = constructPathBytes(for: sourceName, in: sourceDir)
        let destBytes = constructPathBytes(for: destinationName, in: destDir)

        var destNode: UInt64 = 0
        var destParent: UInt64 = 0
        let destResolve = withNullTerminatedCStr(destBytes) { cDest in
            af_resolve_id(fsHandle, getCallingPid(), cDest, &destNode, &destParent)
        }

        if overItem == nil && destResolve == 0 {
            throw fs_errorForPOSIXError(POSIXError.EEXIST.rawValue)
        }

        if let overAgent = overItem as? AgentFsItem, destResolve == 0 {
            let removeResult: Int32 = coreQueue.sync {
                withNullTerminatedCStr(destBytes) { cDest in
                    switch overAgent.attributes.type {
                    case .directory:
                        return af_rmdir(fsHandle, getCallingPid(), cDest)
                    default:
                        return af_unlink(fsHandle, getCallingPid(), cDest)
                    }
                }
            }

            if removeResult != 0, let err = afResultToFSKitError(removeResult) {
                throw err
            }
        }

        let result = coreQueue.sync {
            withNullTerminatedCStr(sourceBytes) { src_cstr in
                withNullTerminatedCStr(destBytes) { dst_cstr in
                    af_rename(fsHandle, getCallingPid(), src_cstr, dst_cstr)
                }
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        agentItem.name = destinationName
        agentItem.pathBytes = destBytes
        agentItem.path = String(decoding: destBytes, as: UTF8.self)
        agentItem.attributes.parentID = destDir.attributes.fileID

        return destinationName
    }

    func enumerateDirectory(
        _ directory: FSItem,
        startingAt cookie: FSDirectoryCookie,
        verifier: FSDirectoryVerifier,
        attributes: FSItem.GetAttributesRequest?,
        packer: FSDirectoryEntryPacker
    ) async throws -> FSDirectoryVerifier {
        guard let directory = directory as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        let dirBytes = directory.pathBytes
        var buffer = [UInt8](repeating: 0, count: 16_384)
        var outLen: size_t = 0
        let result: Int32 = buffer.withUnsafeMutableBytes { bufPtr in
            let byteCount = bufPtr.count
            return coreQueue.sync {
                withNullTerminatedCStr(dirBytes) { cDir in
                    af_readdir(
                        fsHandle,
                        getCallingPid(),
                        cDir,
                        bufPtr.bindMemory(to: CChar.self).baseAddress,
                        byteCount,
                        &outLen
                    )
                }
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            }
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        var entries: [FSFileName] = []
        let total = Int(outLen)
        var offset = 0
        while offset < total {
            var end = offset
            while end < total && buffer[end] != 0 { end += 1 }
            if end > offset {
                let dataSlice = Data(buffer[offset..<end])
                entries.append(FSFileName(data: dataSlice))
            }
            offset = end + 1
        }

        let debugDir = String(decoding: dirBytes, as: UTF8.self)
        logger.debug("enumerateDirectory: found \(entries.count) entries in \(debugDir)")

        var nextCookieValue = cookie.rawValue

        if cookie.rawValue == 0 {
            _ = packer.packEntry(
                name: FSFileName(string: "."),
                itemType: .directory,
                itemID: directory.attributes.fileID,
                nextCookie: FSDirectoryCookie(1),
                attributes: attributes != nil ? directory.attributes : nil
            )
            nextCookieValue = 1
        }

        if cookie.rawValue <= 1 {
            _ = packer.packEntry(
                name: FSFileName(string: ".."),
                itemType: .directory,
                itemID: directory.attributes.parentID,
                nextCookie: FSDirectoryCookie(2),
                attributes: nil
            )
            nextCookieValue = max(nextCookieValue, 2)
        }

        let startIndex = max(0, Int(cookie.rawValue) - 2)
        if startIndex < entries.count {
            for i in startIndex..<entries.count {
                let entryName = entries[i]
                let entryData = entryName.data
                if entryData == Data([0x2e]) || entryData == Data([0x2e, 0x2e]) {
                    continue
                }

                nextCookieValue += 1

                let entryPathBytes = constructPathBytes(for: entryName, in: directory)
                var statBuffer = [UInt8](repeating: 0, count: 64)
                let statResult: Int32 = statBuffer.withUnsafeMutableBytes { statPtr in
                    let statCount = statPtr.count
                    return coreQueue.sync {
                        withNullTerminatedCStr(entryPathBytes) { cPath in
                            af_getattr(
                                fsHandle,
                                getCallingPid(),
                                cPath,
                                statPtr.bindMemory(to: CChar.self).baseAddress,
                                statCount
                            )
                        }
                    }
                }

                var entryType: FSItem.ItemType = .file
                if statResult == 0 && statBuffer.count >= 48 {
                    let fileTypeByte = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 8, as: UInt8.self) }
                    switch fileTypeByte {
                    case 1: entryType = .directory
                    case 2: entryType = .symlink
                    default: entryType = .file
                    }
                }

                var entryAttributes: FSItem.Attributes? = nil
                if attributes != nil && statResult == 0 {
                    let attrs = FSItem.Attributes()
                    let size = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 0, as: UInt64.self) }
                    let mode = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 9, as: UInt32.self) }
                    let atime = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 13, as: Int64.self) }
                    let mtime = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 21, as: Int64.self) }
                    let ctime = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 29, as: Int64.self) }
                    let birthtime = statBuffer.withUnsafeBytes { $0.load(fromByteOffset: 37, as: Int64.self) }
                    attrs.type = entryType
                    attrs.size = size
                    attrs.allocSize = size
                    attrs.mode = mode
                    attrs.parentID = directory.attributes.fileID
                    attrs.accessTime = timespec(tv_sec: Int(atime), tv_nsec: 0)
                    attrs.modifyTime = timespec(tv_sec: Int(mtime), tv_nsec: 0)
                    attrs.changeTime = timespec(tv_sec: Int(ctime), tv_nsec: 0)
                    attrs.birthTime = timespec(tv_sec: Int(birthtime), tv_nsec: 0)
                    entryAttributes = attrs
                }

                var nodeId: UInt64 = 0
                var parentId: UInt64 = 0
                _ = withNullTerminatedCStr(entryPathBytes) { cPath in
                    af_resolve_id(fsHandle, getCallingPid(), cPath, &nodeId, &parentId)
                }

                let packResult = packer.packEntry(
                    name: entryName,
                    itemType: entryType,
                    itemID: FSItem.Identifier(rawValue: nodeId) ?? .invalid,
                    nextCookie: FSDirectoryCookie(nextCookieValue),
                    attributes: entryAttributes
                )

                if !packResult {
                    break
                }
            }
        }

        var hasher = Hasher()
        hasher.combine(dirBytes)
        hasher.combine(entries.count)
        let verifierValue = hasher.finalize()
        return FSDirectoryVerifier(UInt64(bitPattern: Int64(verifierValue)))
    }

    private func mergeAttributes(_ existing: FSItem.Attributes, request: FSItem.SetAttributesRequest) {
        if request.isValid(FSItem.Attribute.uid) {
            existing.uid = request.uid
        }

        if request.isValid(FSItem.Attribute.gid) {
            existing.gid = request.gid
        }

        if request.isValid(FSItem.Attribute.type) {
            existing.type = request.type
        }

        if request.isValid(FSItem.Attribute.mode) {
            existing.mode = request.mode
        }

        if request.isValid(FSItem.Attribute.linkCount) {
            existing.linkCount = request.linkCount
        }

        if request.isValid(FSItem.Attribute.flags) {
            existing.flags = request.flags
        }

        if request.isValid(FSItem.Attribute.size) {
            existing.size = request.size
        }

        if request.isValid(FSItem.Attribute.allocSize) {
            existing.allocSize = request.allocSize
        }

        if request.isValid(FSItem.Attribute.fileID) {
            existing.fileID = request.fileID
        }

        if request.isValid(FSItem.Attribute.parentID) {
            existing.parentID = request.parentID
        }

        if request.isValid(FSItem.Attribute.accessTime) {
            let ts = timespec(tv_sec: Int(request.accessTime.tv_sec), tv_nsec: Int(request.accessTime.tv_nsec))
            existing.accessTime = ts
        }

        if request.isValid(FSItem.Attribute.changeTime) {
            let ts = timespec(tv_sec: Int(request.changeTime.tv_sec), tv_nsec: Int(request.changeTime.tv_nsec))
            existing.changeTime = ts
        }

        if request.isValid(FSItem.Attribute.modifyTime) {
            let ts = timespec(tv_sec: Int(request.modifyTime.tv_sec), tv_nsec: Int(request.modifyTime.tv_nsec))
            existing.modifyTime = ts
        }

        if request.isValid(FSItem.Attribute.addedTime) {
            let ts = timespec(tv_sec: Int(request.addedTime.tv_sec), tv_nsec: Int(request.addedTime.tv_nsec))
            existing.addedTime = ts
        }

        if request.isValid(FSItem.Attribute.birthTime) {
            let ts = timespec(tv_sec: Int(request.birthTime.tv_sec), tv_nsec: Int(request.birthTime.tv_nsec))
            existing.birthTime = ts
        }

        if request.isValid(FSItem.Attribute.backupTime) {
            let ts = timespec(tv_sec: Int(request.backupTime.tv_sec), tv_nsec: Int(request.backupTime.tv_nsec))
            existing.backupTime = ts
        }
    }
}

@available(macOS 15.4, *)
extension AgentFsVolume: FSVolume.OpenCloseOperations {

    func openItem(_ item: FSItem, modes: FSVolume.OpenModes) async throws {
        guard let agentItem = item as? AgentFsItem else {
            logger.debug("open: unknown item type")
            return
        }

        logger.debug("open: \(String(describing: agentItem.name.string ?? "unknown")), modes: \(String(describing: modes)))")

        // Only open handles for regular files
        guard agentItem.attributes.type == .file else {
            return
        }

        let callingPid = getCallingPid()

        // Map FSVolume.OpenModes to options JSON for FFI
        var handle: UInt64 = 0
        let wantsRead = modes.contains(.read)
        let wantsWrite = modes.contains(.write)
        // Honor create/truncate intent when applicable: for FSKit, creation was done in createItem.
        // Keep defaults false here.
        let wantsCreate = false
        let wantsTruncate = false
        let optionsJson = "{\"read\":\(wantsRead),\"write\":\(wantsWrite),\"create\":\(wantsCreate),\"truncate\":\(wantsTruncate)}"

        let result = coreQueue.sync { () -> Int32 in
            return optionsJson.withCString { options_cstr in
                // Prefer opening by node ID to avoid path decoding issues
                let nodeId = agentItem.attributes.fileID.rawValue
                return af_open_by_id(fsHandle, callingPid, nodeId, options_cstr, &handle)
            }
        }

        if result != 0 {
            logger.error("open: failed to open handle for id=\(agentItem.attributes.fileID.rawValue), error: \(result)")
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        let shouldSetUserData = handleStateQueue.sync { () -> Bool in
            handleToPid[handle] = callingPid
            var list = opensByItem[agentItem.attributes.fileID] ?? []
            list.append(handle)
            opensByItem[agentItem.attributes.fileID] = list
            return list.count == 1
        }
        if shouldSetUserData {
            agentItem.userData = handle
        }
        logger.debug("open: opened handle \(handle) for id=\(agentItem.attributes.fileID.rawValue) with PID \(callingPid)")
    }

    func closeItem(_ item: FSItem, modes: FSVolume.OpenModes) async throws {
        guard let agentItem = item as? AgentFsItem else {
            logger.debug("close: unknown item type")
            return
        }

        logger.debug("close: \(String(describing: agentItem.name.string ?? "unknown")), modes: \(String(describing: modes)))")

        // Only close handles for regular files
        guard agentItem.attributes.type == .file else {
            return
        }

        let popped = handleStateQueue.sync { () -> (UInt64, UInt32?, UInt64?)? in
            guard var list = opensByItem[agentItem.attributes.fileID], let h = list.popLast() else {
                return nil
            }
            opensByItem[agentItem.attributes.fileID] = list.isEmpty ? nil : list
            let pid = handleToPid.removeValue(forKey: h)
            let next = list.last
            return (h, pid, next)
        }

        guard let record = popped else {
            logger.debug("close: no handle to close")
            return
        }
        agentItem.userData = record.2

        let pidForHandle = record.1 ?? getCallingPid()
        let result = coreQueue.sync { af_close(fsHandle, pidForHandle, record.0) }

        if result != 0 {
            logger.warning("close: failed to close handle \(record.0), error: \(result)")
            // Don't throw here as the item should still be considered closed
        }
        logger.debug("close: closed handle \(record.0)")
    }
}

@available(macOS 15.4, *)
extension AgentFsVolume: FSVolume.ReadWriteOperations {

    // Async/throws version
    func read(from item: FSItem, at offset: off_t, length: Int, into buffer: FSMutableFileDataBuffer) async throws -> Int {
        guard let agentItem = item as? AgentFsItem else {
            logger.debug("Read operation: unknown item type, offset: \(offset), length: \(length)")
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        logger.debug("Read operation: \(agentItem.name), offset: \(offset), length: \(length)")

        let (handle, pidForHandle, transient) = try obtainHandle(for: agentItem, wantsWrite: false)
        defer {
            if transient {
                _ = coreQueue.sync { af_close(fsHandle, pidForHandle, handle) }
            }
        }

        var bytesRead: UInt32 = 0
        var readData = Data(count: length)
        let result = coreQueue.sync { () -> Int32 in
            return readData.withUnsafeMutableBytes { bufferPtr in
                af_read(fsHandle, pidForHandle, handle, UInt64(offset), bufferPtr.baseAddress, UInt32(length), &bytesRead)
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        // Copy data to the FSKit buffer using the correct method
        let actualBytesRead = Int(bytesRead)
        if actualBytesRead > 0 {
            let dataToCopy = readData.prefix(actualBytesRead)
            _ = dataToCopy.withUnsafeBytes { srcPtr in
                buffer.withUnsafeMutableBytes { dstPtr in
                    memcpy(dstPtr.baseAddress, srcPtr.baseAddress, actualBytesRead)
                }
            }
        }

        return actualBytesRead
    }

    // Async/throws version for write
    func write(contents data: Data, to item: FSItem, at offset: off_t) async throws -> Int {
        guard let agentItem = item as? AgentFsItem else {
            logger.debug("Write operation: unknown item type, offset: \(offset), length: \(data.count)")
            throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
        }

        logger.debug("Write operation: \(agentItem.name), offset: \(offset), length: \(data.count)")

        let (handle, pidForHandle, transient) = try obtainHandle(for: agentItem, wantsWrite: true)
        defer {
            if transient {
                _ = coreQueue.sync { af_close(fsHandle, pidForHandle, handle) }
            }
        }
        var bytesWritten: UInt32 = 0
        let result = coreQueue.sync { () -> Int32 in
            return data.withUnsafeBytes { bufferPtr in
                af_write(fsHandle, pidForHandle, handle, UInt64(offset), bufferPtr.baseAddress, UInt32(data.count), &bytesWritten)
            }
        }

        if result != 0 {
            if let error = afResultToFSKitError(result) {
                throw error
            } else {
                throw fs_errorForPOSIXError(POSIXError.EIO.rawValue)
            }
        }

        let written = Int(bytesWritten)
        // Refresh attributes after write so FSKit sees updated size/times promptly
        do {
            let _ = try fetchAttributesFor(agentItem)
        } catch {
            // ignore best-effort refresh errors
        }
        return written
    }
}

@available(macOS 15.4, *)
extension AgentFsVolume: FSVolume.XattrOperations {

    func xattr(named name: FSFileName, of item: FSItem) async throws -> Data {
        logger.debug("xattr: \(item)")

        guard let agentItem = item as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue)
        }

        var nameBytes = [UInt8](name.data)
        nameBytes.append(0)

        var capacity = 4096
        while true {
            var buffer = [UInt8](repeating: 0, count: capacity)
            var outLen: size_t = 0
            let rc: Int32 = buffer.withUnsafeMutableBytes { bufPtr in
                let byteCount = bufPtr.count
                return coreQueue.sync {
                    withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                        nameBytes.withUnsafeBytes { namePtr in
                            af_xattr_get(
                                fsHandle,
                                getCallingPid(),
                                cPath,
                                namePtr.bindMemory(to: CChar.self).baseAddress,
                                bufPtr.baseAddress,
                                byteCount,
                                &outLen
                            )
                        }
                    }
                }
            }

            if rc == Int32(POSIXError.ERANGE.rawValue) {
                capacity = max(capacity * 2, Int(outLen))
                continue
            }

            if rc != 0, let err = afResultToFSKitError(rc) { throw err }
            return Data(buffer.prefix(Int(outLen)))
        }
    }

    func setXattr(named name: FSFileName, to value: Data?, on item: FSItem, policy: FSVolume.SetXattrPolicy) async throws {
        logger.debug("setXattrOf: \(item)")
        guard let agentItem = item as? AgentFsItem else {
            throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue)
        }
        var nameBytes = [UInt8](name.data)
        nameBytes.append(0)
        let rc: Int32 = coreQueue.sync {
            withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                nameBytes.withUnsafeBytes { namePtr in
                    if let value = value {
                        return value.withUnsafeBytes { bufPtr in
                            af_xattr_set(
                                fsHandle,
                                getCallingPid(),
                                cPath,
                                namePtr.bindMemory(to: CChar.self).baseAddress,
                                bufPtr.baseAddress,
                                bufPtr.count
                            )
                        }
                    } else {
                        return af_xattr_set(
                            fsHandle,
                            getCallingPid(),
                            cPath,
                            namePtr.bindMemory(to: CChar.self).baseAddress,
                            nil,
                            0
                        )
                    }
                }
            }
        }
        if rc != 0, let err = afResultToFSKitError(rc) { throw err }
    }

    func xattrs(of item: FSItem) async throws -> [FSFileName] {
        logger.debug("listXattrs: \(item)")
        guard let agentItem = item as? AgentFsItem else { throw fs_errorForPOSIXError(POSIXError.EINVAL.rawValue) }
        var capacity = 4096
        while true {
            var buffer = [UInt8](repeating: 0, count: capacity)
            var outLen: size_t = 0
            let rc: Int32 = buffer.withUnsafeMutableBytes { bufPtr in
                let byteCount = bufPtr.count
                return coreQueue.sync {
                    withNullTerminatedCStr(agentItem.pathBytes) { cPath in
                        af_xattr_list(
                            fsHandle,
                            getCallingPid(),
                            cPath,
                            bufPtr.baseAddress,
                            byteCount,
                            &outLen
                        )
                    }
                }
            }

            if rc == Int32(POSIXError.ERANGE.rawValue) {
                capacity = max(capacity * 2, Int(outLen))
                continue
            }

            if rc != 0, let err = afResultToFSKitError(rc) { throw err }

            var names: [FSFileName] = []
            let total = Int(outLen)
            var offset = 0
            while offset < total {
                var end = offset
                while end < total && buffer[end] != 0 { end += 1 }
                if end > offset {
                    let dataSlice = Data(buffer[offset..<end])
                    names.append(FSFileName(data: dataSlice))
                }
                offset = end + 1
            }
            return names
        }
    }
}
