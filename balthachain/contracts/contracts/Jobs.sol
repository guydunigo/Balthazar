pragma solidity >=0.4.21 <0.7.0;
pragma experimental ABIEncoderV2;

contract Jobs {
    uint128 public counter;

    function set_counter(uint128 new_val) public {
        // require(counter != new_val, "Same value as before.");
        counter = new_val;
        emit CounterHasNewValue(counter);
    }

    function inc_counter() public {
        counter++;
        emit CounterHasNewValue(counter);
    }
    event CounterHasNewValue(uint128 new_counter);

    enum ProgramKind { Wasm }
    enum BestMethod { Cost, Performance }

    struct Job {
        bytes[] arguments;
        uint64 timeout;
        uint64 max_worker_price;
        uint64 max_network_usage;
        uint64 max_network_price;
        uint64 redundancy;

        uint64 max_failures;

        string data;

        address sender;
        uint128 nonce;

        bool non_null;
    }

    struct Task {
        bytes32 job_id;
        uint128 argument_id;
        bytes result;
        address[] workers;
        uint64[] worker_prices;
        uint64[] network_prices;

        bool non_null;
    }

    struct User {
        bytes32[] pending_jobs;
        // bytes32[] completed_jobs;
        Job[] draft_jobs;
        uint128 next_nonce;
        uint256 locked_money;
        uint256 pending_money;
    }

    mapping(bytes32 => Job) jobs;
    mapping(bytes32 => Task) tasks;
    mapping(address => User) users;

    // ------------------------------------
    // Job and tasks tool functions

    function calc_max_price(Job storage job) internal view returns (uint) {
        return (job.timeout * job.max_worker_price
        + job.max_network_usage * job.max_network_price)
        * job.redundancy * job.arguments.length;
    }

    function calc_job_id(address sender, uint128 nonce) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(sender, nonce));
    }

    function calc_task_id(bytes32 job_id, uint128 index, bytes storage argument) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(job_id, index, argument));
    }

    function is_draft_ready(Job storage job) internal view returns (bool) {
        return job.arguments.length > 0 &&

            job.timeout > 0 &&
            job.max_failures > 0 &&

            job.max_worker_price > 0 &&
            job.max_network_price > 0 &&
            job.redundancy > 0 &&

            users[msg.sender].pending_money >= calc_max_price(job);
    }

    function find_draft_program(uint128 nonce) internal view returns (uint) {
        for (uint i ; i < users[msg.sender].draft_jobs.length ; i++) {
            if (users[msg.sender].draft_jobs[i].nonce == nonce) {
                return i;
            }
        }
        revert("Not found.");
    }

    // ------------------------------------
    // User functions

    function next_nonce_and_increase() internal returns (uint128 nonce) {
        nonce = users[msg.sender].next_nonce;
        users[msg.sender].next_nonce++;
    }

    function get_pending_jobs() public view returns (bytes32[] memory) {
        return users[msg.sender].pending_jobs;
    }
    /*
    function get_completed_jobs() public view returns (bytes32[] memory) {
        return users[msg.sender].completed_jobs;
    }
    */
    function get_draft_jobs() public view returns (uint128[] memory list) {
        Job[] storage draft_jobs = users[msg.sender].draft_jobs;
        list = new uint128[](draft_jobs.length);
        for (uint i; i < draft_jobs.length; i++) {
            list[i] = draft_jobs[i].nonce;
        }
    }
    function get_pending_locked_money() public view returns (uint256, uint256) {
        return (users[msg.sender].pending_money, users[msg.sender].locked_money);
    }
    function send_pending_money() public payable {
        users[msg.sender].pending_money += msg.value;
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);
    }
    // Reverts if there is not enough money in user's pending money.
    function recover_pending_money(uint256 amount) public {
        require(users[msg.sender].pending_money >= amount/*, "too few in pending"*/);

        // prevent re-entrancy attack
        // (See: https://medium.com/@gus_tavo_guim/reentrancy-attack-on-smart-contracts-how-to-identify-the-exploitable-and-an-example-of-an-attack-4470a2d8dfe4)
        users[msg.sender].pending_money -= amount;
        msg.sender.transfer(amount);
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);
    }

    // ------------------------------------
    // Job manipulation functions

    function create_job() public {
        uint128 nonce = next_nonce_and_increase();
        users[msg.sender].draft_jobs.push(Job(
            new bytes[](0),
            1,
            1,
            0,
            1,
            1,
            0,
            new string(0),
            msg.sender,
            nonce,
            true
        ));
        emit JobNew(msg.sender, nonce);
    }

    function get_parameters_draft(uint128 nonce) public view returns (
        // ProgramKind,
        uint64,
        uint64,
        uint64
    ) {
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        return (
            j.timeout,
            j.max_failures,
            j.redundancy
        );
    }
    // Reverts if `timeout` or `max_failures` is null.
    function set_parameters_draft(
        uint128 nonce,
        uint64 timeout,
        uint64 max_failures,
        uint64 redundancy// ,
    ) public {
        require(timeout > 0 && redundancy > 0/*, "invalid data"*/);
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        j.timeout = timeout;
        j.max_failures = max_failures;
        j.redundancy = redundancy;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return (
            jobs[job_id].timeout,
            jobs[job_id].max_failures,
            jobs[job_id].redundancy
        );
    }

    function get_data_draft(uint128 nonce) public view returns (string memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].data;
    }
    function set_data_draft(uint128 nonce, string memory val) public {
        users[msg.sender].draft_jobs[find_draft_program(nonce)].data = val;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_data(bytes32 job_id) public view returns (string memory) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return jobs[job_id].data;
    }

    function get_arguments_draft(uint128 nonce) public view returns (bytes[] memory) {
        return users[msg.sender].draft_jobs[find_draft_program(nonce)].arguments;
    }
    // Reverts if the provided array is empty.
    function set_arguments_draft(uint128 nonce, bytes[] memory val) public {
        require(val.length > 0/*, "empty array"*/);
        users[msg.sender].draft_jobs[find_draft_program(nonce)].arguments = val;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_arguments(bytes32 job_id) public view returns (bytes[] memory) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return jobs[job_id].arguments;
    }

    function get_worker_parameters_draft(uint128 nonce) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        return (
            j.max_worker_price,
            j.max_network_usage,
            j.max_network_price
        );
    }
    // Reverts if `max_worker_price` or `max_network_price` is null.
    function set_worker_parameters_draft(
        uint128 nonce,
        uint64 max_worker_price,
        uint64 max_network_usage,
        uint64 max_network_price
    ) public {
        require(max_worker_price > 0 && max_network_price > 0/*, "invalid data"*/);
        Job storage j = users[msg.sender].draft_jobs[find_draft_program(nonce)];

        j.max_worker_price = max_worker_price;
        j.max_network_usage = max_network_usage;
        j.max_network_price = max_network_price;
    }
    // Reverts if there is no job corresponding to `job_id`.
    function get_worker_parameters(bytes32 job_id) public view returns (
        uint64,
        uint64,
        uint64
    ) {
        require(jobs[job_id].non_null/*, "unknown job"*/);

        return (
            jobs[job_id].max_worker_price,
            jobs[job_id].max_network_usage,
            jobs[job_id].max_network_price// ,
        );
    }

    // Reverts if there is no job corresponding to `job_id`.
    function get_sender_nonce(bytes32 job_id) public view returns (address, uint128) {
        require(jobs[job_id].non_null/*, "unknown job"*/);
        return (jobs[job_id].sender, jobs[job_id].nonce);
    }

    // Reverts if there is no draft job corresponding to `id`.
    function delete_draft(uint id) internal returns (Job memory job) {
        User storage user = users[msg.sender];
        require(user.draft_jobs.length > id/*, "unknown id"*/);

        job = user.draft_jobs[id];

        if (user.draft_jobs.length > id + 1) {
            user.draft_jobs[id] = user.draft_jobs[user.draft_jobs.length - 1];
        }

        user.draft_jobs.pop();
    }

    function delete_draft_nonce(uint128 nonce) public {
        delete_draft(find_draft_program(nonce));
    }

    function ready(uint128 nonce) public {
        Job storage job = users[msg.sender].draft_jobs[find_draft_program(nonce)];
        require(is_draft_ready(job), "conditions unmet");

        bytes32 job_id = calc_job_id(job.sender, job.nonce);
        require(jobs[job_id].non_null == false, "job collision");

        // TODO: a lot of copies ? Directly store the job in jobs from the beginning?
        jobs[job_id] = job;
        delete_draft(find_draft_program(nonce));
        job = jobs[job_id];

        users[msg.sender].pending_jobs.push(job_id);
        emit JobPending(job_id);

        for (uint128 i; i < job.arguments.length ; i++) {
            bytes32 task_id = calc_task_id(job_id, i, job.arguments[i]);
            require(tasks[task_id].non_null == false, "task collision");
            tasks[task_id] = Task(job_id,
                                   i,
                                   new bytes(0),
                                   new address[](0),
                                   new uint64[](0),
                                   new uint64[](0),
                                   true);
            emit TaskPending(task_id);
        }

        uint max_price = calc_max_price(job);
        users[msg.sender].pending_money -= max_price;
        users[msg.sender].locked_money += max_price;
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);
    }

    // ------------------------------------
    // Function that require proper consensus.

    function set_result(bytes32 task_id, bytes memory result, address[] memory workers, uint64[] memory worker_prices, uint64[] memory network_prices) public {
        require(tasks[task_id].non_null, "unknown task");
        require(tasks[task_id].result.length == 0, "already completed task");
        require(result.length > 0, "empty result");
        require(workers.length == worker_prices.length && workers.length == network_prices.length, "not same sizes");
        require(workers.length == jobs[tasks[task_id].job_id].redundancy, "incorrect length");

        tasks[task_id].result = result;
        tasks[task_id].workers = workers;
        tasks[task_id].worker_prices = worker_prices;
        tasks[task_id].network_prices = network_prices;
        emit NewResult(task_id, result);
    }

    // Reverts if there is no task corresponding to `task_id`.
    function get_result(bytes32 task_id) public view returns (bytes memory) {
        require(tasks[task_id].non_null/*, "unknown task"*/);
        return tasks[task_id].result;
    }

    // Reverts if there is no job corresponding to `job_id`.
    function is_job_completed(bytes32 job_id) public view returns (bool result) {
        require(jobs[job_id].non_null/*, "unknown job"*/);

        result = true;
        for (uint128 i ; i < jobs[job_id].arguments.length ; i++) {
            if (tasks[calc_task_id(job_id, i, jobs[job_id].arguments[i])].result.length == 0) {
                result = false;
            }
        }
    }

    // Accepts the results, pays the workers and unlock the remaining money for the sender
    // to recover.
    function validate_results(bytes32 job_id) public {
        require(jobs[job_id].non_null, "unknown job");
        require(msg.sender == jobs[job_id].sender, "not sender");
        require(is_job_completed(job_id), "incomplete job");

        uint total_amount;

        for (uint128 i ; i < jobs[job_id].arguments.length ; i++) {
            Task storage task = tasks[calc_task_id(job_id, i, jobs[job_id].arguments[i])];
            for (uint64 j ; j < task.workers.length ; j++) {
                uint amount = task.worker_prices[j] * jobs[job_id].timeout
                   + task.network_prices[j] * jobs[job_id].max_network_usage;
                users[task.workers[j]].pending_money += amount;
                total_amount += amount;
                emit PendingMoneyChanged(task.workers[j], users[task.workers[j]].pending_money);
            }
        }
        uint max_price = calc_max_price(jobs[job_id]);
        users[msg.sender].locked_money -= max_price;
        users[msg.sender].pending_money += max_price - total_amount;
        emit PendingMoneyChanged(msg.sender, users[msg.sender].pending_money);

        bytes32[] storage pending_jobs = users[msg.sender].pending_jobs;
        uint i;
        for (; i < pending_jobs.length ; i++) {
            if (pending_jobs[i] == job_id) {
                break;
            }
        }
        if (pending_jobs.length - 1 > i) {
            pending_jobs[i] = pending_jobs[pending_jobs.length - 1];
        }
        pending_jobs.pop();
    }

    // Reverts if there is no task corresponding to `task_id`.
    function get_task(bytes32 task_id) public view returns (bytes32, uint128, bytes memory) {
        Task storage task = tasks[task_id];
        require(task.non_null/*, "unknown task"*/);
        return (
            task.job_id,
            task.argument_id,
            jobs[task.job_id].arguments[task.argument_id]
        );
    }

    // ------------------------------------------------------------
    // Events

    event JobNew(address sender, uint128 nonce);
    event JobPending(bytes32 job_id);
    event TaskPending(bytes32 task_id);
    event NewResult(bytes32 task_id, bytes result);
    event PendingMoneyChanged(address account, uint new_val);
}
