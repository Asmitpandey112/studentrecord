#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

// ─── Data Types ──────────────────────────────────────────────────────────────

/// Stores all information about a single student.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Student {
    pub student_id: u64,
    pub name: String,
    pub age: u32,
    pub course: String,
    pub grades: Vec<u32>, // 0–100 per subject
    pub enrolled: bool,
}

/// Keys used in contract storage.
#[contracttype]
pub enum DataKey {
    Student(u64),   // student_id → Student
    NextId,         // auto-increment counter
    Admin,          // contract administrator
}

// ─── Events ──────────────────────────────────────────────────────────────────

const EVT_ENROLLED: Symbol  = symbol_short!("ENROLLED");
const EVT_UPDATED:  Symbol  = symbol_short!("UPDATED");
const EVT_REMOVED:  Symbol  = symbol_short!("REMOVED");

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct StudentRecordContract;

#[contractimpl]
impl StudentRecordContract {

    // ── Admin / Initialisation ────────────────────────────────────────────

    /// Must be called once right after deployment.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialised");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::NextId, &1u64);
    }

    // ── Write Operations ──────────────────────────────────────────────────

    /// Enrol a new student. Returns the auto-assigned student ID.
    pub fn enroll_student(
        env: Env,
        caller: Address,
        name: String,
        age: u32,
        course: String,
    ) -> u64 {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let id: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);

        let student = Student {
            student_id: id,
            name,
            age,
            course,
            grades: Vec::new(&env),
            enrolled: true,
        };

        env.storage().persistent().set(&DataKey::Student(id), &student);
        env.storage().instance().set(&DataKey::NextId, &(id + 1));

        env.events().publish((EVT_ENROLLED,), id);

        id
    }

    /// Add a grade (0–100) for an existing, enrolled student.
    pub fn add_grade(env: Env, caller: Address, student_id: u64, grade: u32) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        if grade > 100 {
            panic!("grade must be 0-100");
        }

        let mut student: Student = Self::get_student_or_panic(&env, student_id);
        if !student.enrolled {
            panic!("student is not enrolled");
        }

        student.grades.push_back(grade);
        env.storage().persistent().set(&DataKey::Student(student_id), &student);
        env.events().publish((EVT_UPDATED,), student_id);
    }

    /// Update a student's course.
    pub fn update_course(env: Env, caller: Address, student_id: u64, new_course: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut student: Student = Self::get_student_or_panic(&env, student_id);
        student.course = new_course;
        env.storage().persistent().set(&DataKey::Student(student_id), &student);
        env.events().publish((EVT_UPDATED,), student_id);
    }

    /// Unenrol a student (soft delete – data is preserved).
    pub fn unenroll_student(env: Env, caller: Address, student_id: u64) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let mut student: Student = Self::get_student_or_panic(&env, student_id);
        student.enrolled = false;
        env.storage().persistent().set(&DataKey::Student(student_id), &student);
        env.events().publish((EVT_REMOVED,), student_id);
    }

    // ── Read Operations ───────────────────────────────────────────────────

    /// Fetch the full record for a student.
    pub fn get_student(env: Env, student_id: u64) -> Student {
        Self::get_student_or_panic(&env, student_id)
    }

    /// Return the average grade (0 if no grades recorded).
    pub fn get_average_grade(env: Env, student_id: u64) -> u32 {
        let student: Student = Self::get_student_or_panic(&env, student_id);
        let count = student.grades.len();
        if count == 0 {
            return 0;
        }
        let mut total: u64 = 0;
        for g in student.grades.iter() {
            total += g as u64;
        }
        (total / count as u64) as u32
    }

    /// Returns true if the student is currently enrolled.
    pub fn is_enrolled(env: Env, student_id: u64) -> bool {
        match env.storage().persistent().get::<DataKey, Student>(&DataKey::Student(student_id)) {
            Some(s) => s.enrolled,
            None => false,
        }
    }

    /// Return the total number of students ever enrolled (including unenrolled).
    pub fn total_students(env: Env) -> u64 {
        let next: u64 = env.storage().instance().get(&DataKey::NextId).unwrap_or(1);
        next - 1
    }

    // ── Internal Helpers ──────────────────────────────────────────────────

    fn get_student_or_panic(env: &Env, student_id: u64) -> Student {
        env.storage()
            .persistent()
            .get(&DataKey::Student(student_id))
            .unwrap_or_else(|| panic!("student not found"))
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("contract not initialised");
        if *caller != admin {
            panic!("only the admin can call this function");
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, StudentRecordContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, StudentRecordContract);
        let client = StudentRecordContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        (env, client, admin)
    }

    #[test]
    fn test_enroll_and_fetch() {
        let (env, client, admin) = setup();
        let id = client.enroll_student(
            &admin,
            &String::from_str(&env, "Alice"),
            &20,
            &String::from_str(&env, "Computer Science"),
        );
        assert_eq!(id, 1);
        let s = client.get_student(&id);
        assert_eq!(s.name, String::from_str(&env, "Alice"));
        assert!(s.enrolled);
    }

    #[test]
    fn test_grades_and_average() {
        let (env, client, admin) = setup();
        let id = client.enroll_student(
            &admin,
            &String::from_str(&env, "Bob"),
            &22,
            &String::from_str(&env, "Math"),
        );
        client.add_grade(&admin, &id, &80);
        client.add_grade(&admin, &id, &90);
        client.add_grade(&admin, &id, &70);
        assert_eq!(client.get_average_grade(&id), 80);
    }

    #[test]
    fn test_unenroll() {
        let (env, client, admin) = setup();
        let id = client.enroll_student(
            &admin,
            &String::from_str(&env, "Carol"),
            &21,
            &String::from_str(&env, "Physics"),
        );
        client.unenroll_student(&admin, &id);
        assert!(!client.is_enrolled(&id));
    }

    #[test]
    fn test_total_students() {
        let (env, client, admin) = setup();
        client.enroll_student(&admin, &String::from_str(&env, "Dave"),  &19, &String::from_str(&env, "Art"));
        client.enroll_student(&admin, &String::from_str(&env, "Eve"),   &20, &String::from_str(&env, "Music"));
        client.enroll_student(&admin, &String::from_str(&env, "Frank"), &23, &String::from_str(&env, "Law"));
        assert_eq!(client.total_students(), 3);
    }
}