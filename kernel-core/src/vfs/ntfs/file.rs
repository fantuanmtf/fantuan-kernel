//! NTFS $DATA reads (M12-4): resident values and non-resident streams read
//! through the decoded runlist, with sparse runs zero-filled and bytes past
//! the initialized size zero-filled. The bounded cluster cache carries a
//! sequential read across calls.

use super::record::{AttrValue, Record};
use super::{cache, Ntfs, NtfsErr};

impl Ntfs {
    /// Read the record's unnamed $DATA at byte OFFSET into OUT. Returns the
    /// bytes copied (0 at or past EOF).
    pub fn read_at(&self, rec: &Record, offset: u64, out: &mut [u8]) -> Result<usize, NtfsErr> {
        match rec.data()? {
            Some(AttrValue::Resident(v)) => {
                if offset >= v.len() as u64 {
                    return Ok(0);
                }
                let start = offset as usize;
                let n = (v.len() - start).min(out.len());
                out[..n].copy_from_slice(&v[start..start + n]);
                Ok(n)
            }
            Some(AttrValue::NonResident(nr)) => self.read_stream(&nr, offset, out),
            None => {
                if rec.has_attr_list() {
                    Err(NtfsErr::AttrList)
                } else if rec.is_dir() {
                    Err(NtfsErr::NotDir)
                } else {
                    Err(NtfsErr::NotFound)
                }
            }
        }
    }

    fn read_stream(
        &self,
        nr: &super::record::NonResident,
        offset: u64,
        out: &mut [u8],
    ) -> Result<usize, NtfsErr> {
        if offset >= nr.data_size {
            return Ok(0);
        }
        let want = out.len().min((nr.data_size - offset) as usize);
        let init_end = nr.init_size.min(nr.data_size);
        let cs = self.cluster_size as u64;
        let mut done = 0usize;
        while done < want {
            let pos = offset + done as u64;
            if pos >= init_end {
                out[done..want].fill(0);
                return Ok(want);
            }
            let vcn = pos / cs;
            let in_cluster = (pos % cs) as usize;
            let n = (self.cluster_size as usize - in_cluster)
                .min(want - done)
                .min((init_end - pos) as usize);
            if n == 0 {
                return Err(NtfsErr::Corrupt);
            }
            match nr.runs.lcn_of(vcn)? {
                Some(lcn) => {
                    let lba = self.part_lba + lcn * self.sectors_per_cluster as u64;
                    if !cache::read_cluster(lba, self.cluster_size as usize, in_cluster, &mut out[done..done + n]) {
                        return Err(NtfsErr::Io);
                    }
                }
                None => out[done..done + n].fill(0),
            }
            done += n;
        }
        Ok(done)
    }

    /// Read bytes at OFFSET of a run-backed stream (MFT, index allocation):
    /// arbitrary offsets spanning runs, sparse runs zero-filled.
    pub fn read_runs_at(&self, runs: &super::runlist::RunList, offset: u64, out: &mut [u8]) -> Result<usize, NtfsErr> {
        let cs = self.cluster_size as u64;
        let mut done = 0usize;
        while done < out.len() {
            let pos = offset.checked_add(done as u64).ok_or(NtfsErr::RunList)?;
            let vcn = pos / cs;
            let in_cluster = (pos % cs) as usize;
            let n = (self.cluster_size as usize - in_cluster).min(out.len() - done);
            match runs.lcn_of(vcn)? {
                Some(lcn) => {
                    let lba = self.part_lba + lcn * self.sectors_per_cluster as u64;
                    if !cache::read_cluster(lba, self.cluster_size as usize, in_cluster, &mut out[done..done + n]) {
                        return Err(NtfsErr::Io);
                    }
                }
                None => out[done..done + n].fill(0),
            }
            done += n;
        }
        Ok(done)
    }
}
