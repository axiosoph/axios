@0xb8d8f0d996dfe9b0;

struct PlanDigest {
  bytes @0 :Data;  # 32-byte Blake3 digest
}

struct BuildStatus {
  union {
    queued @0 :Void;
    evaluating :group { message @1 :Text; }
    building :group { phase @2 :Text; progress @3 :Float32; }
    completed :group { outputPaths @4 :List(Text); outputDigest @5 :Data; }
    failed :group { error @6 :Text; exitCode @7 :Int32; }
    cancelled @8 :Void;
  }
}

interface ProgressStream {
  update @0 (status :BuildStatus) -> stream;
  done @1 () -> ();
}

struct AtomSetEntry {
  anchor @0 :Text;
  tag @1 :Text;
  mirrors @2 :List(Text);
}

struct DepDescriptor {
  union {
    atom :group {
      id @0 :AtomId;
      label @1 :Text;
      version @2 :Text;
      set @3 :Text;
      rev @4 :Text;
      requires @5 :List(AtomId);
      direct @6 :Bool;
    }
    nix :group {
      name @7 :Text;
      url @8 :Text;
      hash @9 :Text;
      owner @10 :AtomId;
    }
    nixGit :group {
      name @11 :Text;
      url @12 :Text;
      rev @13 :Text;
      version @14 :Text;
      owner @15 :AtomId;
    }
    nixTar :group {
      name @16 :Text;
      url @17 :Text;
      hash @18 :Text;
      owner @19 :AtomId;
    }
    nixSrc :group {
      name @20 :Text;
      url @21 :Text;
      hash @22 :Text;
      owner @23 :AtomId;
    }
  }
}

struct ComposerSpec {
  union {
    atom :group {
      id @0 :AtomId;
      entry @1 :Text;
      args @2 :List(KeyValue);
    }
    nixTrivial :group {
      expression @3 :Text;
      args @4 :List(KeyValue);
    }
    static @5 :Void;
  }
}

struct BuildRequest {
  planDigest @0 :Data;
  sets @1 :List(AtomSetEntry);
  deps @2 :List(DepDescriptor);
  composer @3 :ComposerSpec;
  evalArgs @4 :List(KeyValue);
}

interface EosDaemon {
  submitBuild @0 (request :BuildRequest) -> (job :BuildJob);
  queryStatus @1 (jobId :Data) -> (status :BuildStatus);
  getCapabilities @2 () -> (
    supportedBackends :List(Text),
    apiVersion :UInt32
  );
  discover @3 () -> (discovery :AtomDiscovery);
}

interface BuildJob {
  attachProgress @0 (callback :ProgressStream) -> ();
  cancel @1 () -> ();
  getJobId @2 () -> (jobId :Data);
  getMissing @3 () -> (missingAtoms :List(AtomId));
}

interface AtomDiscovery {
  resolve @0 (id :AtomId) -> (meta :AtomMeta);
  contains @1 (id :AtomId) -> (exists :Bool);
  search @2 (query :AtomQuery) -> (results :List(AtomMeta));
}

struct AtomId {
  digest @0 :Data;
}

struct AtomMeta {
  id @0 :AtomId;
  label @1 :Text;
  versions @2 :List(VersionInfo);
  sets @3 :List(Text);  # anchor hashes of sets containing this atom
}

struct VersionInfo {
  version @0 :Text;
  rev @1 :Text;
  set @2 :Text;
}

struct AtomQuery {
  labelPattern @0 :Text;    # glob or substring match
  setFilter @1 :Text;       # optional: restrict to specific set
  limit @2 :UInt32;         # max results
}

struct KeyValue {
  key @0 :Text;
  value @1 :Text;
}
